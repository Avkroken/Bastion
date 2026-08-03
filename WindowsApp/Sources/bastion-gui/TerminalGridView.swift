import SwiftCrossUI

/// Renderar `TerminalBuffer` som en textrutnät. SwiftCrossUI saknar en
/// ritbar Canvas, så varje rad byggs av hopslagna körningar av celler med
/// samma stil (färg/vikt) i stället för en `Text`-vy per cell — annars
/// blir det tusentals vyer per bildruta.
struct TerminalGridView: View {
    // Ingen egen observation här — SwiftCrossUI har bara @State, inget
    // @ObservedObject. Föräldern (TerminalSessionView) håller `controller`
    // i @State och äger observationen; se `@Published var buffer` i
    // TerminalController (TerminalSessionView.swift).
    let buffer: TerminalBuffer

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(buffer.rows.enumerated()), id: \.offset) { rowIndex, row in
                terminalRow(row, rowIndex: rowIndex)
            }
        }
        .padding(6)
        .background(Color.black)
    }

    private func terminalRow(_ row: [TerminalCell], rowIndex: Int) -> some View {
        HStack(spacing: 0) {
            ForEach(runs(for: row, rowIndex: rowIndex), id: \.id) { run in
                Text(run.text)
                    .font(.system(size: 13, design: .monospaced).emphasized(run.bold))
                    .foregroundColor(run.fg)
                    .background(run.bg)
            }
        }
    }

    private struct Run: Identifiable {
        let id: Int
        let text: String
        let fg: Color
        let bg: Color
        let bold: Bool
    }

    /// Slår ihop intilliggande celler med identisk stil till en `Text`-körning.
    /// Markören visas genom att invertera fg/bg för just den cellen.
    private func runs(for row: [TerminalCell], rowIndex: Int) -> [Run] {
        var result: [Run] = []
        var currentText = ""
        var currentFg = Color.white
        var currentBg = Color.black
        var currentBold = false
        var started = false
        var runID = 0

        func flush() {
            guard started else { return }
            result.append(Run(id: runID, text: currentText, fg: currentFg, bg: currentBg, bold: currentBold))
            runID += 1
            currentText = ""
        }

        for (colIndex, cell) in row.enumerated() {
            let isCursor = rowIndex == buffer.cursorRow && colIndex == buffer.cursorCol
            var fg = cell.fg ?? Color.white
            var bg = cell.bg ?? Color.black
            if isCursor { swap(&fg, &bg) }
            if started && fg == currentFg && bg == currentBg && cell.bold == currentBold {
                currentText.append(cell.char)
            } else {
                flush()
                currentFg = fg
                currentBg = bg
                currentBold = cell.bold
                currentText = String(cell.char)
                started = true
            }
        }
        flush()
        return result
    }
}
