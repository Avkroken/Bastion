import Foundation

/// Tangentsekvenserna en terminal förväntar sig för tangenter ett
/// mjukvarutangentbord inte har.
///
/// VISION:s iPhone-tangentbordssektion räknar upp Ctrl, Esc, Tab, pilar,
/// Alt och F1–F12. Ingen av dem finns på iOS systemtangentbord, och utan
/// dem går det inte att avbryta ett kommando, komplettera en sökväg eller
/// ta sig ur `vi`.
///
/// # Varför det här ligger i SSHCore och inte i vyn
///
/// Sekvenserna är ren data med exakta värden, och exaktheten är hela
/// poängen — `ESC [ 1 8 ~` är F7, `ESC [ 1 9 ~` är F8, och en etta fel
/// betyder att fel tangent når servern. Som data i kärnan går de att
/// testa i `swiftpm-linux`; som strängar inne i en SwiftUI-vy hade de
/// bara kunnat provas för hand på en enhet.
public enum TerminalKeys {
    public static let escape: [UInt8] = [0x1B]
    public static let tab: [UInt8] = [0x09]
    public static let enter: [UInt8] = [0x0D]
    public static let backspace: [UInt8] = [0x7F]

    /// Piltangenter i NORMALT läge (`ESC [ A`).
    ///
    /// Program som `vi` och `less` slår om terminalen till
    /// application cursor keys och väntar sig `ESC O A` i stället. Den
    /// omställningen ägs av terminalemulatorn, som ser växlingen i
    /// dataströmmen — därför skickar den här tabellen alltid normalläget,
    /// och SwiftTerm översätter vid behov. Att gissa läget här hade
    /// betytt två källor till sanning om samma tillstånd.
    public enum Arrow: String, CaseIterable {
        case up = "A", down = "B", right = "C", left = "D"

        public var bytes: [UInt8] {
            [0x1B, 0x5B] + Array(rawValue.utf8)
        }
    }

    public enum Navigation: CaseIterable {
        case home, end, pageUp, pageDown, insert, delete

        public var bytes: [UInt8] {
            switch self {
            case .home: return [0x1B, 0x5B, 0x48]           // ESC [ H
            case .end: return [0x1B, 0x5B, 0x46]            // ESC [ F
            case .pageUp: return [0x1B, 0x5B, 0x35, 0x7E]   // ESC [ 5 ~
            case .pageDown: return [0x1B, 0x5B, 0x36, 0x7E] // ESC [ 6 ~
            case .insert: return [0x1B, 0x5B, 0x32, 0x7E]   // ESC [ 2 ~
            case .delete: return [0x1B, 0x5B, 0x33, 0x7E]   // ESC [ 3 ~
            }
        }
    }

    /// `ESC [ … ~`-numren för F5–F12 är INTE löpande.
    ///
    /// 16 och 22 hoppas över — en historisk kvarleva från DEC:s
    /// tangentbord som varje terminal ändå följer. Att räkna `14 + n` ger
    /// rätt svar för F5 och F6 och sedan fel för allt därefter, vilket är
    /// precis den sortens bugg som ser ut att fungera vid en snabb
    /// kontroll.
    private static let functionTildeCodes: [Int: Int] = [
        5: 15, 6: 17, 7: 18, 8: 19, 9: 20, 10: 21, 11: 23, 12: 24,
    ]

    /// F1–F12. `nil` för allt utanför intervallet.
    ///
    /// F1–F4 använder en helt annan form (`ESC O P`) än F5 och uppåt
    /// (`ESC [ 1 5 ~`). Det är inte inkonsekvens i den här koden utan i
    /// standarden.
    public static func function(_ number: Int) -> [UInt8]? {
        switch number {
        case 1...4:
            // ESC O P/Q/R/S
            let letter = UInt8(0x50 + number - 1)
            return [0x1B, 0x4F, letter]
        case 5...12:
            guard let code = functionTildeCodes[number] else { return nil }
            return [0x1B, 0x5B] + Array(String(code).utf8) + [0x7E]
        default:
            return nil
        }
    }

    /// Ctrl + tecken, alltså kontrollkoden 0x00–0x1F.
    ///
    /// Regeln är att de fem översta bitarna nollställs: `Ctrl+A` blir
    /// 0x01, `Ctrl+C` 0x03. Den gäller bokstäver samt `@ [ \ ] ^ _` och
    /// mellanslag; allt annat har ingen kontrollkod och ger `nil` i
    /// stället för en påhittad byte.
    public static func control(_ character: Character) -> [UInt8]? {
        // ASCII-kontrollen görs FÖRST, före versalisering.
        //
        // `Character(character.uppercased())` ser oskyldigt ut men kraschar:
        // `ß`.uppercased() är `SS`, alltså två tecken, och `Character(_:)`
        // kräver exakt ett. Ett tangenttryck ska aldrig kunna fälla appen.
        guard let raw = character.asciiValue else { return nil }
        // Versal och gemen ger samma kontrollkod — Ctrl+c och Ctrl+C är
        // samma tangenttryck för en terminal. Inom ASCII räcker en
        // bitmask för det.
        let ascii = (0x61...0x7A).contains(raw) ? raw - 0x20 : raw
        switch ascii {
        case 0x40...0x5F: // @ A-Z [ \ ] ^ _
            return [ascii & 0x1F]
        case 0x20: // mellanslag = Ctrl+@ = NUL
            return [0x00]
        case 0x3F: // ? = DEL, den enda utanför masken
            return [0x7F]
        default:
            return nil
        }
    }

    /// Väntande modifierare på en pekskärm.
    ///
    /// Ctrl och Alt finns inte att HÅLLA NED på ett mjukvarutangentbord —
    /// man kan bara trycka. Därför är de klibbiga: ett tryck arm:ar dem,
    /// nästa tecken förbrukar dem. Samma modell som iOS eget
    /// skift-beteende, och den enda som fungerar med en tumme.
    ///
    /// Logiken ligger här och inte i vyn för att den har ett par kanter
    /// värda att låsa fast: att trycka Ctrl två gånger ska stänga av den
    /// igen (inte arm:a dubbelt), och Ctrl+Alt+tecken ska ge ESC följt av
    /// kontrollkoden — inte tvärtom, och inte bara den ena.
    public struct Modifiers: Equatable {
        public var control: Bool
        public var alt: Bool

        public init(control: Bool = false, alt: Bool = false) {
            self.control = control
            self.alt = alt
        }

        public var isEmpty: Bool { !control && !alt }

        /// Ett tryck på modifierartangenten växlar den.
        public mutating func toggleControl() { control.toggle() }
        public mutating func toggleAlt() { alt.toggle() }

        public mutating func clear() {
            control = false
            alt = false
        }
    }

    /// Löser upp ett tecken mot väntande modifierare och NOLLSTÄLLER dem.
    ///
    /// `nil` betyder att kombinationen inte finns — `Ctrl+1` har ingen
    /// kontrollkod — och då ska ingenting skickas. Att falla tillbaka på
    /// det omodifierade tecknet vore värre än att inte göra något: `1` i
    /// stället för `Ctrl+1` skriver in en etta i det man höll på med.
    ///
    /// Modifierarna nollställs ÄVEN när resultatet blir `nil`. Annars
    /// blir en misslyckad kombination en fälla där nästa tecken plötsligt
    /// modifieras utan att användaren begärt det.
    public static func resolve(
        character: Character,
        modifiers: inout Modifiers
    ) -> [UInt8]? {
        defer { modifiers.clear() }

        switch (modifiers.control, modifiers.alt) {
        case (false, false):
            return Array(String(character).utf8)
        case (true, false):
            return control(character)
        case (false, true):
            return alt(character)
        case (true, true):
            // ESC följt av kontrollkoden. Ordningen är inte godtycklig:
            // Meta uttrycks som ett ESC-prefix FÖRE tecknet, och tecknet
            // är i det här fallet redan kontrollkodat.
            guard let ctrl = control(character) else { return nil }
            return [0x1B] + ctrl
        }
    }

    /// Alt (Meta) + tecken skickas som ESC följt av tecknet.
    ///
    /// Det är "ESC-prefix"-konventionen, som är vad readline och alla
    /// vanliga skal förväntar sig — inte den äldre varianten där åttonde
    /// biten sätts, som bryter mot UTF-8.
    public static func alt(_ character: Character) -> [UInt8] {
        [0x1B] + Array(String(character).utf8)
    }
}
