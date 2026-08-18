import XCTest
@testable import SSHCore

final class CommandLibraryTests: XCTestCase {
    func testAllEntriesHaveUniqueIDs() {
        let ids = CommandLibrary.all.map(\.id)
        XCTAssertEqual(ids.count, Set(ids).count, "dubbletter av (kategori, kommando) hittades")
    }

    func testAllCategoriesRepresented() {
        // VISION.md: "Docker, Linux, Git, Cloudflare, Tailscale, WireGuard, systemd"
        let represented = Set(CommandLibrary.all.map(\.category))
        XCTAssertEqual(represented, Set(CommandLibraryEntry.Category.allCases))
    }

    func testEntriesFilteredByCategory() {
        let docker = CommandLibrary.entries(in: .docker)
        XCTAssertFalse(docker.isEmpty)
        XCTAssertTrue(docker.allSatisfy { $0.category == .docker })
    }

    func testNoEntryIsEmpty() {
        for entry in CommandLibrary.all {
            XCTAssertFalse(entry.command.trimmingCharacters(in: .whitespaces).isEmpty, entry.id)
            XCTAssertFalse(entry.summary.trimmingCharacters(in: .whitespaces).isEmpty, entry.id)
        }
    }

    /// VISION-kravet som regel i stället för som ambition. Fem av trettio
    /// poster hade dokumentationslänk innan den här kördes första gången.
    func testEveryEntryCarriesDocumentation() {
        for entry in CommandLibrary.all {
            guard let url = entry.docsURL else {
                XCTFail("\(entry.id) saknar dokumentationslänk")
                continue
            }
            XCTAssertTrue(url.hasPrefix("https://"), "\(entry.id): länken ska vara https, är \(url)")
        }
    }

    /// Ett exempel finns för att visa hur en variabel fylls i. Har
    /// kommandot inga variabler blir exemplet en upprepning av kommandot
    /// — därför gäller kravet mallarna, och bara dem.
    func testTemplatedCommandsCarryAnExampleWithTheVariablesFilledIn() {
        for entry in CommandLibrary.all where entry.command.contains("{{") {
            guard let example = entry.example else {
                XCTFail("\(entry.id) är en mall men saknar exempel")
                continue
            }
            XCTAssertFalse(
                example.contains("{{"),
                "\(entry.id): exemplet ska visa ifyllda värden, inte mallen igen (\(example))"
            )
        }
    }

    func testAsSnippetRendersVariablesLikeARealSnippet() {
        let entry = CommandLibraryEntry(category: .docker, command: "docker compose restart {{service}}", summary: "test")
        XCTAssertEqual(entry.asSnippet.variableNames, ["service"])
        XCTAssertEqual(entry.asSnippet.rendered(with: ["service": "plex"]), "docker compose restart plex")
    }
}
