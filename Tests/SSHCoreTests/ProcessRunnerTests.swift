import Foundation
import XCTest
@testable import SSHCore

#if !os(iOS) && !os(tvOS) && !os(watchOS)
/// `ProcessRunner` är den sista SSHCore-modulen som saknade tester. Den
/// delas av `TailscaleStatus.fetchLocal` och `BitwardenClient`, så ett
/// fel här slår mot båda.
///
/// Det viktigaste testet är dödläget. Modulens egen kommentar förklarar
/// varför stdout och stderr MÅSTE läsas konkurrent, men förklaringen var
/// det enda som skyddade den: ingen kontrollerade att det faktiskt
/// gjordes. Den som "förenklar" tillbaka till sekventiell läsning får nu
/// ett test som hänger i stället för en kommentar som ignoreras.
final class ProcessRunnerTests: XCTestCase {

    private let shell = URL(fileURLWithPath: "/bin/sh")

    func testCapturesStdoutStderrAndExitCodeSeparately() throws {
        let result = try ProcessRunner.run(
            executableURL: shell,
            arguments: ["-c", "printf hej; printf fel 1>&2; exit 3"]
        )
        XCTAssertEqual(String(decoding: result.stdout, as: UTF8.self), "hej")
        XCTAssertEqual(String(decoding: result.stderr, as: UTF8.self), "fel")
        XCTAssertEqual(result.exitCode, 3, "exitkoden ska nå fram, inte slukas")
    }

    func testSuccessfulCommandReportsZero() throws {
        let result = try ProcessRunner.run(executableURL: shell, arguments: ["-c", "true"])
        XCTAssertEqual(result.exitCode, 0)
        XCTAssertTrue(result.stdout.isEmpty)
        XCTAssertTrue(result.stderr.isEmpty)
    }

    /// Kärnan: mer på stderr än OS:ets pipebuffert rymmer (~64 KiB på
    /// Linux). Läses strömmarna sekventiellt blockerar barnet på write()
    /// till stderr medan vi väntar på stdout — ingen sida kan gå vidare,
    /// och testet hänger tills XCTest ger upp.
    ///
    /// 200 000 byte är valt att ligga klart över gränsen på varje
    /// plattform vi bygger för, inte precis på den.
    func testLargeStderrDoesNotDeadlock() throws {
        let size = 200_000
        let result = try ProcessRunner.run(
            executableURL: shell,
            arguments: ["-c", "head -c \(size) /dev/zero | tr '\\0' 'x' 1>&2"]
        )
        XCTAssertEqual(result.stderr.count, size, "hela stderr ska ha lästs")
        XCTAssertEqual(result.exitCode, 0)
    }

    /// Samma sak åt andra hållet — stor stdout medan stderr är tom.
    func testLargeStdoutIsReadInFull() throws {
        let size = 200_000
        let result = try ProcessRunner.run(
            executableURL: shell,
            arguments: ["-c", "head -c \(size) /dev/zero | tr '\\0' 'y'"]
        )
        XCTAssertEqual(result.stdout.count, size)
        XCTAssertEqual(result.exitCode, 0)
    }

    /// Och båda samtidigt, vilket är det verkliga fallet: ett kommando som
    /// skriver mycket på båda strömmarna.
    func testBothStreamsLargeAtOnce() throws {
        let size = 120_000
        let result = try ProcessRunner.run(
            executableURL: shell,
            arguments: [
                "-c",
                "head -c \(size) /dev/zero | tr '\\0' 'a' & head -c \(size) /dev/zero | tr '\\0' 'b' 1>&2; wait",
            ]
        )
        XCTAssertEqual(result.stdout.count, size)
        XCTAssertEqual(result.stderr.count, size)
    }

    /// Miljön ska ÄRVAS och kompletteras, inte ersättas. Ersätts den
    /// försvinner PATH och HOME, och varje integration som anropar ett
    /// verktyg via namn slutar fungera.
    func testEnvironmentIsMergedNotReplaced() throws {
        let result = try ProcessRunner.run(
            executableURL: shell,
            arguments: ["-c", "printf '%s|%s' \"$BASTION_TEST_VAR\" \"${PATH:+harPATH}\""],
            environment: ["BASTION_TEST_VAR": "satt"]
        )
        let text = String(decoding: result.stdout, as: UTF8.self)
        XCTAssertEqual(text, "satt|harPATH", "PATH ska finnas kvar bredvid den tillagda variabeln")
    }

    /// En binär som inte finns ska kasta, inte returnera en tyst nolla.
    func testMissingExecutableThrows() {
        XCTAssertThrowsError(
            try ProcessRunner.run(
                executableURL: URL(fileURLWithPath: "/finns/inte/alls"),
                arguments: []
            )
        )
    }
}
#endif
