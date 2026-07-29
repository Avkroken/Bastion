import SwiftCrossUI

/// En cell i terminalskärmen: tecken + färg/vikt.
struct TerminalCell: Equatable {
    var char: Character = " "
    var fg: Color?
    var bg: Color?
    var bold = false
}

/// Minimal VT100/ANSI-tolk: skärmbuffert (rader × kolumner), markörposition,
/// CSI-sekvenser för markörflytt/radering/färg (SGR). Ingen scrollregion,
/// ingen alt-screen, ingen bredare charset-hantering — täcker det vanligaste
/// (bash-prompt, `ls --color`, `docker ps`, `git status`, loggutskrifter).
///
/// Bygger på `feed(_:)` per mottagen SSH-chunk; tolkningsläget (`state`) bevaras
/// mellan anrop så en escape-sekvens som delas mellan två chunkar ändå tolkas rätt.
@MainActor
final class TerminalBuffer: ObservableObject {
    let cols: Int
    let rowCount: Int
    private(set) var rows: [[TerminalCell]]
    private(set) var cursorRow = 0
    private(set) var cursorCol = 0
    /// Bumpas en gång per `feed(_:)`-anrop — vyn observerar den här, inte
    /// `rows` direkt, så att en hel chunk tolkas innan SwiftCrossUI ritar om.
    @Published private(set) var revision = 0

    private enum ParseState { case ground, esc, csi, osc }
    private var state: ParseState = .ground
    private var csiBuf = ""

    private var currentFg: Color?
    private var currentBg: Color?
    private var currentBold = false

    private static let ansiPalette: [Color] = [
        Color(red: 0.0, green: 0.0, blue: 0.0),
        Color(red: 0.75, green: 0.15, blue: 0.15),
        Color(red: 0.15, green: 0.65, blue: 0.15),
        Color(red: 0.75, green: 0.65, blue: 0.15),
        Color(red: 0.20, green: 0.35, blue: 0.85),
        Color(red: 0.65, green: 0.20, blue: 0.65),
        Color(red: 0.15, green: 0.65, blue: 0.70),
        Color(red: 0.75, green: 0.75, blue: 0.75),
        Color(red: 0.40, green: 0.40, blue: 0.40),
        Color(red: 1.00, green: 0.35, blue: 0.35),
        Color(red: 0.35, green: 0.90, blue: 0.35),
        Color(red: 1.00, green: 0.90, blue: 0.35),
        Color(red: 0.45, green: 0.55, blue: 1.00),
        Color(red: 0.95, green: 0.40, blue: 0.95),
        Color(red: 0.35, green: 0.90, blue: 0.90),
        Color(white: 1.0),
    ]

    init(cols: Int, rows: Int) {
        self.cols = cols
        self.rowCount = rows
        self.rows = Array(repeating: Self.blankRow(cols: cols), count: rows)
    }

    private static func blankRow(cols: Int) -> [TerminalCell] {
        Array(repeating: TerminalCell(), count: cols)
    }

    // Itererar Unicode.Scalar, INTE Character: Swift grupperar "\r\n" till EN
    // enda grafemkluster-Character, så en per-Character-switch matchar aldrig
    // lös \r eller \n i en CRLF-sekvens — precis så de flesta skal skickar
    // radslut. Per-scalar undviker det helt, och är dessutom mer korrekt för
    // en terminalemulator (som är kodpunktsorienterad, inte grafemklusterorienterad).
    func feed(_ text: String) {
        for scalar in text.unicodeScalars { process(scalar) }
        revision &+= 1
    }

    private func process(_ scalar: Unicode.Scalar) {
        switch state {
        case .ground:
            switch scalar {
            case "\u{1B}": state = .esc
            case "\r": cursorCol = 0
            case "\n": newline()
            case "\u{08}": cursorCol = max(0, cursorCol - 1)
            case "\t": cursorCol = min(cols - 1, ((cursorCol / 8) + 1) * 8)
            default:
                if scalar.value < 0x20 { break } // annat styrtecken (bell m.m.) — ignoreras
                writeChar(Character(scalar))
            }
        case .esc:
            if scalar == "[" { state = .csi; csiBuf = "" }
            else if scalar == "]" { state = .osc }
            else { state = .ground } // ostödd enstaka esc-sekvens (charset m.m.)
        case .osc:
            if scalar == "\u{07}" { state = .ground } // BEL avslutar OSC (t.ex. fönstertitel)
        case .csi:
            if (Unicode.Scalar("0")...Unicode.Scalar("9")).contains(scalar) || scalar == ";" {
                csiBuf.append(Character(scalar))
            } else {
                handleCSI(final: Character(scalar), paramString: csiBuf)
                state = .ground
            }
        }
    }

    private func writeChar(_ ch: Character) {
        guard cursorRow < rowCount else { return }
        if cursorCol >= cols { cursorCol = 0; newline() }
        rows[cursorRow][cursorCol] = TerminalCell(char: ch, fg: currentFg, bg: currentBg, bold: currentBold)
        cursorCol += 1
    }

    private func newline() {
        cursorRow += 1
        if cursorRow >= rowCount {
            rows.removeFirst()
            rows.append(Self.blankRow(cols: cols))
            cursorRow = rowCount - 1
        }
    }

    private func handleCSI(final: Character, paramString: String) {
        let params = paramString.split(separator: ";", omittingEmptySubsequences: false).map { Int($0) ?? 0 }
        let p0 = params.first ?? 0
        switch final {
        case "A": cursorRow = max(0, cursorRow - max(1, p0))
        case "B": cursorRow = min(rowCount - 1, cursorRow + max(1, p0))
        case "C": cursorCol = min(cols - 1, cursorCol + max(1, p0))
        case "D": cursorCol = max(0, cursorCol - max(1, p0))
        case "H", "f":
            let row = params.count > 0 && params[0] > 0 ? params[0] : 1
            let col = params.count > 1 && params[1] > 0 ? params[1] : 1
            cursorRow = min(max(0, row - 1), rowCount - 1)
            cursorCol = min(max(0, col - 1), cols - 1)
        case "J": eraseDisplay(p0)
        case "K": eraseLine(p0)
        case "m": applySGR(params.isEmpty ? [0] : params)
        default: break // ostödd CSI-final (t.ex. scrollregion, markörsynlighet) — ignoreras
        }
    }

    private func eraseDisplay(_ mode: Int) {
        switch mode {
        case 1:
            for r in 0..<cursorRow { rows[r] = Self.blankRow(cols: cols) }
            if rows.indices.contains(cursorRow) {
                for c in 0...min(cursorCol, cols - 1) { rows[cursorRow][c] = TerminalCell() }
            }
        case 2, 3:
            rows = Array(repeating: Self.blankRow(cols: cols), count: rowCount)
        default:
            if rows.indices.contains(cursorRow) {
                for c in cursorCol..<cols { rows[cursorRow][c] = TerminalCell() }
            }
            if cursorRow + 1 < rowCount {
                for r in (cursorRow + 1)..<rowCount { rows[r] = Self.blankRow(cols: cols) }
            }
        }
    }

    private func eraseLine(_ mode: Int) {
        guard rows.indices.contains(cursorRow) else { return }
        switch mode {
        case 1: for c in 0...min(cursorCol, cols - 1) { rows[cursorRow][c] = TerminalCell() }
        case 2: rows[cursorRow] = Self.blankRow(cols: cols)
        default: for c in cursorCol..<cols { rows[cursorRow][c] = TerminalCell() }
        }
    }

    /// 256-färgspaletten (SGR `38;5;n`/`48;5;n`): 0-15 är samma som
    /// standard-/ljusa ANSI-färgerna, 16-231 är en 6×6×6-RGB-kub, 232-255 är
    /// en gråskale-ramp — samma indelning som xterm och alla vanliga
    /// terminalemulatorer använder.
    private static func color256(_ n: Int) -> Color {
        switch n {
        case 0..<8: return ansiPalette[n]
        case 8..<16: return ansiPalette[n]
        case 16..<232:
            let i = n - 16
            let steps: [Double] = [0, 95, 135, 175, 215, 255]
            let r = steps[(i / 36) % 6] / 255
            let g = steps[(i / 6) % 6] / 255
            let b = steps[i % 6] / 255
            return Color(red: r, green: g, blue: b)
        default:
            let level = 8 + (n - 232) * 10
            let v = Double(min(level, 255)) / 255
            return Color(red: v, green: v, blue: v)
        }
    }

    /// Läser en 256-färg- (`5;n`) eller True Color- (`2;r;g;b`) sekvens ur
    /// `params` från och med index `i` (som pekar på `38`/`48`). Returnerar
    /// färgen och hur många ytterligare parametrar som konsumerades, eller
    /// `nil` om sekvensen är trasig/ofullständig (params slut i förtid) —
    /// då hoppas resten av den enskilda SGR-sekvensen bara över istället för
    /// att krascha på ett array-index utanför gränserna.
    private static func extendedColor(_ params: [Int], from i: Int) -> (Color, Int)? {
        guard i + 1 < params.count else { return nil }
        switch params[i + 1] {
        case 5:
            guard i + 2 < params.count else { return nil }
            return (color256(params[i + 2]), 2)
        case 2:
            guard i + 4 < params.count else { return nil }
            let r = Double(params[i + 2]) / 255
            let g = Double(params[i + 3]) / 255
            let b = Double(params[i + 4]) / 255
            return (Color(red: r, green: g, blue: b), 4)
        default:
            return nil
        }
    }

    private func applySGR(_ params: [Int]) {
        var i = 0
        while i < params.count {
            let p = params[i]
            switch p {
            case 0: currentFg = nil; currentBg = nil; currentBold = false
            case 1: currentBold = true
            case 22: currentBold = false
            case 30...37: currentFg = Self.ansiPalette[p - 30]
            case 38:
                if let (color, consumed) = Self.extendedColor(params, from: i) {
                    currentFg = color
                    i += consumed
                }
            case 39: currentFg = nil
            case 40...47: currentBg = Self.ansiPalette[p - 40]
            case 48:
                if let (color, consumed) = Self.extendedColor(params, from: i) {
                    currentBg = color
                    i += consumed
                }
            case 49: currentBg = nil
            case 90...97: currentFg = Self.ansiPalette[8 + (p - 90)]
            case 100...107: currentBg = Self.ansiPalette[8 + (p - 100)]
            default: break // kursiv/understrykning m.m. — ignoreras i v1
            }
            i += 1
        }
    }
}
