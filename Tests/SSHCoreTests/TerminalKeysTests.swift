import XCTest
@testable import SSHCore

/// Sekvenserna är ren data med exakta värden, och exaktheten är hela
/// poängen: en etta fel betyder att fel tangent når servern. Testerna
/// jämför mot xterms dokumenterade koder, inte mot vad koden råkar göra.
final class TerminalKeysTests: XCTestCase {

    func testArrowsUseNormalCursorMode() {
        XCTAssertEqual(TerminalKeys.Arrow.up.bytes, [0x1B, 0x5B, 0x41])    // ESC [ A
        XCTAssertEqual(TerminalKeys.Arrow.down.bytes, [0x1B, 0x5B, 0x42])  // ESC [ B
        XCTAssertEqual(TerminalKeys.Arrow.right.bytes, [0x1B, 0x5B, 0x43]) // ESC [ C
        XCTAssertEqual(TerminalKeys.Arrow.left.bytes, [0x1B, 0x5B, 0x44])  // ESC [ D

        // Alla fyra ska vara olika — en delad rawValue hade gett samma
        // sekvens för två riktningar.
        let all = Set(TerminalKeys.Arrow.allCases.map { $0.bytes })
        XCTAssertEqual(all.count, 4)
    }

    /// F1–F4 har en HELT annan form än F5 och uppåt. Det är standarden som
    /// är inkonsekvent, inte koden.
    func testFunctionKeysOneThroughFourUseTheSS3Form() {
        XCTAssertEqual(TerminalKeys.function(1), [0x1B, 0x4F, 0x50]) // ESC O P
        XCTAssertEqual(TerminalKeys.function(2), [0x1B, 0x4F, 0x51]) // ESC O Q
        XCTAssertEqual(TerminalKeys.function(3), [0x1B, 0x4F, 0x52]) // ESC O R
        XCTAssertEqual(TerminalKeys.function(4), [0x1B, 0x4F, 0x53]) // ESC O S
    }

    /// Kärnan i hela filen: tilde-numren är INTE löpande. 16 och 22
    /// hoppas över, och en naiv `14 + n` ger rätt för F5 och F6 och sedan
    /// fel för allt därefter.
    func testFunctionKeyTildeNumbersSkipSixteenAndTwentyTwo() {
        func expect(_ n: Int, _ code: String) {
            let want: [UInt8] = [0x1B, 0x5B] + Array(code.utf8) + [0x7E]
            XCTAssertEqual(TerminalKeys.function(n), want, "F\(n) ska vara ESC [ \(code) ~")
        }
        expect(5, "15")
        expect(6, "17")   // inte 16
        expect(7, "18")
        expect(8, "19")
        expect(9, "20")
        expect(10, "21")
        expect(11, "23")  // inte 22
        expect(12, "24")

        // Den naiva formeln skulle gett 16 för F6 — kontrollera att den
        // sekvensen inte produceras av någon tangent alls.
        let naiveF6: [UInt8] = [0x1B, 0x5B, 0x31, 0x36, 0x7E]
        for n in 1...12 {
            XCTAssertNotEqual(TerminalKeys.function(n), naiveF6)
        }
    }

    func testFunctionKeysOutsideTheRangeAreNilNotAGuess() {
        XCTAssertNil(TerminalKeys.function(0))
        XCTAssertNil(TerminalKeys.function(13))
        XCTAssertNil(TerminalKeys.function(-1))
        XCTAssertNil(TerminalKeys.function(100))
    }

    /// Alla tolv ska vara unika. Två tangenter som ger samma bytes är en
    /// bugg som bara märks när någon trycker på just den andra.
    func testEveryFunctionKeyIsDistinct() {
        let all = (1...12).compactMap { TerminalKeys.function($0) }
        XCTAssertEqual(all.count, 12)
        XCTAssertEqual(Set(all).count, 12)
    }

    func testControlMasksTheTopBitsAndIgnoresCase() {
        XCTAssertEqual(TerminalKeys.control("a"), [0x01])
        XCTAssertEqual(TerminalKeys.control("A"), [0x01], "Ctrl+c och Ctrl+C är samma tangenttryck")
        XCTAssertEqual(TerminalKeys.control("c"), [0x03])
        XCTAssertEqual(TerminalKeys.control("d"), [0x04])
        XCTAssertEqual(TerminalKeys.control("z"), [0x1A])
        XCTAssertEqual(TerminalKeys.control("["), [0x1B], "Ctrl+[ är samma sak som Esc")
        XCTAssertEqual(TerminalKeys.control("\\"), [0x1C])
        XCTAssertEqual(TerminalKeys.control("_"), [0x1F])
        XCTAssertEqual(TerminalKeys.control(" "), [0x00], "Ctrl+mellanslag är NUL")
        XCTAssertEqual(TerminalKeys.control("?"), [0x7F])
    }

    /// Tecken utan kontrollkod ska ge nil, inte en påhittad byte. En
    /// felaktig byte hade skickats till servern och tolkats som något
    /// annat.
    func testCharactersWithoutAControlCodeReturnNil() {
        for c: Character in ["1", "å", "€", "!", "~"] {
            XCTAssertNil(TerminalKeys.control(c), "\(c) har ingen kontrollkod")
        }
    }

    /// `ß`.uppercased() är `SS` — två tecken. En versalisering före
    /// ASCII-kontrollen hade fällt appen på ett tangenttryck.
    func testCharactersWhoseUppercaseIsLongerDoNotCrash() {
        XCTAssertNil(TerminalKeys.control("ß"))
        XCTAssertNil(TerminalKeys.control("ﬁ"))
        XCTAssertNil(TerminalKeys.control("😀"))
    }

    /// ESC-prefix, inte den åttonde biten — den senare bryter UTF-8.
    func testAltUsesEscapePrefixAndSurvivesNonAscii() {
        XCTAssertEqual(TerminalKeys.alt("b"), [0x1B, 0x62])
        XCTAssertEqual(TerminalKeys.alt("f"), [0x1B, 0x66])
        XCTAssertEqual(TerminalKeys.alt("."), [0x1B, 0x2E])

        // Ett tecken utanför ASCII ska bli ESC + dess UTF-8-bytes, inte
        // trunkeras till en byte.
        XCTAssertEqual(TerminalKeys.alt("å"), [0x1B] + Array("å".utf8))
    }

    func testNavigationKeysMatchXterm() {
        XCTAssertEqual(TerminalKeys.Navigation.home.bytes, [0x1B, 0x5B, 0x48])
        XCTAssertEqual(TerminalKeys.Navigation.end.bytes, [0x1B, 0x5B, 0x46])
        XCTAssertEqual(TerminalKeys.Navigation.pageUp.bytes, [0x1B, 0x5B, 0x35, 0x7E])
        XCTAssertEqual(TerminalKeys.Navigation.pageDown.bytes, [0x1B, 0x5B, 0x36, 0x7E])
        XCTAssertEqual(TerminalKeys.Navigation.delete.bytes, [0x1B, 0x5B, 0x33, 0x7E])

        let all = Set(TerminalKeys.Navigation.allCases.map { $0.bytes })
        XCTAssertEqual(all.count, TerminalKeys.Navigation.allCases.count)
    }

    func testSingleByteKeysAreTheControlCodesNotLetters() {
        XCTAssertEqual(TerminalKeys.escape, [0x1B])
        XCTAssertEqual(TerminalKeys.tab, [0x09])
        XCTAssertEqual(TerminalKeys.enter, [0x0D])
        XCTAssertEqual(TerminalKeys.backspace, [0x7F], "terminaler väntar DEL, inte BS")
    }
}

/// Klibbiga modifierare — Ctrl och Alt går inte att HÅLLA NED på en
/// pekskärm, så de arm:as med ett tryck och förbrukas av nästa tecken.
final class TerminalModifierTests: XCTestCase {

    func testPlainCharacterPassesThroughUnchanged() {
        var mods = TerminalKeys.Modifiers()
        XCTAssertEqual(TerminalKeys.resolve(character: "a", modifiers: &mods), [0x61])
        XCTAssertEqual(TerminalKeys.resolve(character: "å", modifiers: &mods), Array("å".utf8))
    }

    func testArmedModifierIsConsumedByExactlyOneCharacter() {
        var mods = TerminalKeys.Modifiers()
        mods.toggleControl()
        XCTAssertTrue(mods.control)

        XCTAssertEqual(TerminalKeys.resolve(character: "c", modifiers: &mods), [0x03])
        XCTAssertTrue(mods.isEmpty, "modifieraren ska vara förbrukad")

        // Nästa tecken är omodifierat.
        XCTAssertEqual(TerminalKeys.resolve(character: "c", modifiers: &mods), [0x63])
    }

    /// Att trycka Ctrl två gånger ska stänga av den igen, inte arm:a
    /// dubbelt — annars går den inte att ångra utan att skicka något.
    func testTappingTheSameModifierTwiceTurnsItOff() {
        var mods = TerminalKeys.Modifiers()
        mods.toggleControl()
        mods.toggleControl()
        XCTAssertTrue(mods.isEmpty)
        XCTAssertEqual(TerminalKeys.resolve(character: "c", modifiers: &mods), [0x63])
    }

    /// ESC-prefixet kommer FÖRE kontrollkoden, inte efter.
    func testControlAndAltCombineAsEscapeThenControlCode() {
        var mods = TerminalKeys.Modifiers(control: true, alt: true)
        XCTAssertEqual(TerminalKeys.resolve(character: "c", modifiers: &mods), [0x1B, 0x03])
        XCTAssertTrue(mods.isEmpty)
    }

    func testAltAloneIsEscapePrefix() {
        var mods = TerminalKeys.Modifiers(alt: true)
        XCTAssertEqual(TerminalKeys.resolve(character: "b", modifiers: &mods), [0x1B, 0x62])
    }

    /// En omöjlig kombination ska ge nil — INTE det omodifierade tecknet.
    /// `1` i stället för `Ctrl+1` skriver in en etta i det man höll på med.
    func testImpossibleCombinationSendsNothingRatherThanThePlainCharacter() {
        var mods = TerminalKeys.Modifiers(control: true)
        XCTAssertNil(TerminalKeys.resolve(character: "1", modifiers: &mods))
        XCTAssertTrue(mods.isEmpty, "även ett misslyckat försök ska nollställa")

        // Och den nollställningen betyder att nästa tecken är rent.
        XCTAssertEqual(TerminalKeys.resolve(character: "1", modifiers: &mods), [0x31])
    }
}
