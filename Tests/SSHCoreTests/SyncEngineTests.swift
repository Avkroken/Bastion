import Foundation
import XCTest
@testable import SSHCore

private typealias Host = SSHCore.Host   // undvik krock med Foundation.Host

final class SyncEngineTests: XCTestCase {
    private func host(_ id: UUID, _ alias: String, at t: TimeInterval) -> Host {
        Host(id: id, alias: alias, hostName: "h", user: "u",
             modifiedAt: Date(timeIntervalSince1970: t))
    }

    func testUnionOfDistinctHosts() {
        let a = SyncState(hosts: [host(UUID(), "a", at: 1)])
        let b = SyncState(hosts: [host(UUID(), "b", at: 1)])
        let m = SyncEngine.merge(a, b)
        XCTAssertEqual(m.hosts.map { $0.alias }, ["a", "b"])
    }

    func testLastWriteWins() {
        let id = UUID()
        let a = SyncState(hosts: [host(id, "gammal", at: 100)])
        let b = SyncState(hosts: [host(id, "ny", at: 200)])
        XCTAssertEqual(SyncEngine.merge(a, b).hosts.first?.alias, "ny")
        XCTAssertEqual(SyncEngine.merge(b, a).hosts.first?.alias, "ny")  // ordningsoberoende
    }

    func testTombstoneDeletesAcrossDevices() {
        let id = UUID()
        // Enhet A raderade (gravsten senare än värdens ändring), enhet B har kvar den.
        let a = SyncState(tombstones: [id: Date(timeIntervalSince1970: 300)])
        let b = SyncState(hosts: [host(id, "kvar", at: 200)])
        let m = SyncEngine.merge(a, b)
        XCTAssertTrue(m.hosts.isEmpty)
        XCTAssertNotNil(m.tombstones[id])
    }

    func testNewerEditRevivesOverOlderDelete() {
        let id = UUID()
        let a = SyncState(tombstones: [id: Date(timeIntervalSince1970: 100)])
        let b = SyncState(hosts: [host(id, "återupplivad", at: 200)])   // redigerad efter raderingen
        let m = SyncEngine.merge(a, b)
        XCTAssertEqual(m.hosts.map { $0.alias }, ["återupplivad"])
        XCTAssertNil(m.tombstones[id])
    }

    func testIdempotentAndCommutative() {
        let id1 = UUID(), id2 = UUID()
        let a = SyncState(hosts: [host(id1, "a", at: 10)], tombstones: [id2: Date(timeIntervalSince1970: 5)])
        let b = SyncState(hosts: [host(id2, "b", at: 3), host(id1, "a2", at: 20)])
        let ab = SyncEngine.merge(a, b)
        let ba = SyncEngine.merge(b, a)
        XCTAssertEqual(ab, ba)                                  // kommutativt
        XCTAssertEqual(SyncEngine.merge(ab, ab), ab)            // idempotent
        XCTAssertEqual(ab.hosts.map { $0.alias }, ["a2"])       // LWW + gravsten slår b:s äldre id2
    }

    func testStoreMergePersists() {
        let local = HostStore(path: nil)
        let shared = Host(id: UUID(), alias: "delad", hostName: "h", user: "u")
        // Fjärrenhet har en värd vi inte har.
        local.merge(SyncState(hosts: [shared]))
        XCTAssertEqual(local.get(shared.id)?.alias, "delad")
    }

    // Två enheter som synkar genom en delad mapp konvergerar — inkl. radering.
    func testTwoDevicesConvergeThroughSharedFolder() throws {
        let dir = NSTemporaryDirectory() + "bastion-sync-\(ProcessInfo.processInfo.processIdentifier)"
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let provider = FolderSyncProvider(path: dir + "/shared.json")
        let deviceA = HostStore(path: dir + "/a.json")
        let deviceB = HostStore(path: dir + "/b.json")
        // Egna, tomma snippet-databaser: testet handlar om värdar, och den
        // enda synkvägen tar båda.
        let snipsA = SnippetStore(path: dir + "/a-snippets.json")
        let snipsB = SnippetStore(path: dir + "/b-snippets.json")

        let h = Host(id: UUID(), alias: "web", hostName: "1.1.1.1", user: "u")
        deviceA.upsert(h)
        try deviceA.sync(with: provider, snippets: snipsA)                 // A skjuter upp
        try deviceB.sync(with: provider, snippets: snipsB)                 // B hämtar
        XCTAssertEqual(deviceB.get(h.id)?.alias, "web")

        deviceB.delete(h.id)                             // B raderar
        try deviceB.sync(with: provider, snippets: snipsB)
        try deviceA.sync(with: provider, snippets: snipsA)                 // A hämtar raderingen
        XCTAssertNil(deviceA.get(h.id))
    }

    /// `tombstones` är ett handskrivet kontrakt mot LinuxApp. Den här sidan
    /// kodar `[UUID: Date]` som en PLATT array av omväxlande nycklar och
    /// värden — inte som ett JSON-objekt — eftersom `UUID` inte är
    /// `CodingKeyRepresentable`. LinuxApp har egna `Serialize`/`Deserialize`
    /// vars enda syfte är att härma just det.
    ///
    /// Går formen isär tappas gravstenarna, och en tappad gravsten betyder
    /// att en RADERAD värd återuppstår vid nästa synk. Det ser ut som ett
    /// spöke i gränssnittet och aldrig som ett serialiseringsproblem.
    ///
    /// LinuxApp läser samma fil i
    /// `host::tests::the_shared_sync_state_fixture_decodes_to_the_expected_state`.
    func testTheSharedSyncStateFixtureDecodesIdentically() throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()      // SSHCoreTests
            .deletingLastPathComponent()      // Tests
            .appendingPathComponent("fixtures/sync-state-wire-format.json")
        let data = try Data(contentsOf: fixture)
        let state = try JSONDecoder().decode(SyncState.self, from: data)

        // Toppnivåns nycklar är ett kontrakt precis som postens egna. Går de
        // isär avvisas eller tappas HELA synken, inte bara ett fält.
        let raw = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        XCTAssertEqual(
            raw.keys.sorted(), ["hosts", "snippets", "tombstones"],
            "SyncStates toppnivå ändrades. Uppdatera LinuxApps serde-kod i samma veva.")

        XCTAssertEqual(state.hosts.count, 1)
        XCTAssertEqual(state.hosts.first?.alias, "synk")
        XCTAssertEqual(state.snippets.count, 1, "snippets ingår i tillståndet")
        XCTAssertEqual(state.snippets.first?.name, "starta om plex")
        XCTAssertEqual(state.snippets.first?.template, "docker compose restart {{tjanst}}")
        XCTAssertEqual(state.tombstones.count, 2, "den platta arrayen ska ge TVÅ gravstenar")

        let first = try XCTUnwrap(UUID(uuidString: "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee"))
        let second = try XCTUnwrap(UUID(uuidString: "99999999-8888-7777-6666-555555555555"))
        XCTAssertEqual(
            state.tombstones[first]?.timeIntervalSinceReferenceDate ?? -1, 780_000_000,
            accuracy: 0.001)
        XCTAssertEqual(
            state.tombstones[second]?.timeIntervalSinceReferenceDate ?? -1, 785_000_000,
            accuracy: 0.001)

        // Det vi SKRIVER måste ha samma platta form, annars kan LinuxApp inte
        // läsa det vi just bevisat att vi kan läsa.
        let written = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(state)) as! [String: Any]
        let flat = try XCTUnwrap(written["tombstones"] as? [Any])
        XCTAssertEqual(flat.count, 4, "två gravstenar = fyra element, inte ett objekt")
        XCTAssertTrue(flat[0] is String, "nyckel, värde, nyckel, värde")
    }

    // MARK: - Snippets i synken

    private func snippet(_ id: UUID, _ name: String, at t: TimeInterval) -> Snippet {
        Snippet(
            id: id, name: name, template: "echo \(name)",
            modifiedAt: Date(timeIntervalSince1970: t))
    }

    /// Snippets följer EXAKT samma regler som värdar — det är hela poängen med
    /// att hopslagningen är utbruten i stället för kopierad.
    func testSnippetsFollowTheSameLastWriteWinsRuleAsHosts() {
        let id = UUID()
        let merged = SyncEngine.merge(
            SyncState(snippets: [snippet(id, "gammal", at: 10)]),
            SyncState(snippets: [snippet(id, "ny", at: 20)]))
        XCTAssertEqual(merged.snippets.first?.name, "ny")
    }

    /// En raderad snippet får INTE återuppstå. Gravstenarna delar karta med
    /// värdarna, och en hopslagning som bara behåller gravstenar utan levande
    /// VÄRD skulle tyst kasta varje gravsten som hörde till en snippet.
    func testADeletedSnippetStaysDeletedThroughAMerge() {
        let id = UUID()
        let merged = SyncEngine.merge(
            SyncState(snippets: [snippet(id, "raderad", at: 10)]),
            SyncState(tombstones: [id: Date(timeIntervalSince1970: 30)]))
        XCTAssertTrue(merged.snippets.isEmpty, "gravstenen ska vinna över den äldre versionen")
        XCTAssertNotNil(
            merged.tombstones[id],
            "gravstenen måste ÖVERLEVA hopslagningen, annars återuppstår snippeten nästa varv")

        let again = SyncEngine.merge(merged, merged)
        XCTAssertTrue(again.snippets.isEmpty)
        XCTAssertNotNil(again.tombstones[id])
    }

    /// En nyare redigering återupplivar en snippet, precis som för värdar.
    func testANewerSnippetEditRevivesItOverAnOlderTombstone() {
        let id = UUID()
        let merged = SyncEngine.merge(
            SyncState(tombstones: [id: Date(timeIntervalSince1970: 10)]),
            SyncState(snippets: [snippet(id, "aterupplivad", at: 20)]))
        XCTAssertEqual(merged.snippets.count, 1)
        XCTAssertNil(merged.tombstones[id], "gravstenen ska falla för den nyare ändringen")
    }

    /// Ett tomt snippet-fält får inte kasta bort motpartens snippets.
    func testMergingAgainstAPeerWithoutSnippetsKeepsOurs() {
        let mine = SyncState(snippets: [snippet(UUID(), "min", at: 10)])
        XCTAssertEqual(SyncEngine.merge(mine, SyncState()).snippets.count, 1)
        XCTAssertEqual(SyncEngine.merge(SyncState(), mine).snippets.count, 1, "och åt andra hållet")
    }

    /// Buggen den här sidan hade kvar efter att LinuxApp och WindowsApp
    /// rättats: `>=` lät den sist besökta kopian vinna på en EXAKT
    /// tidsstämpel-krock, så merge(a, b) och merge(b, a) gav olika svar. Två
    /// enheter som synkar mot varandra hade då aldrig konvergerat på just den
    /// posten — de skulle byta värde med varandra i all evighet.
    func testMergeIsCommutativeEvenOnAnExactModifiedAtTie() {
        let id = UUID()
        let a = SyncState(hosts: [host(id, "alpha", at: 42)])
        let b = SyncState(hosts: [host(id, "bravo", at: 42)])
        XCTAssertEqual(
            SyncEngine.merge(a, b).hosts.first?.alias,
            SyncEngine.merge(b, a).hosts.first?.alias,
            "samma par måste ge samma vinnare oavsett ordning")
    }

    /// Samma krav för snippets.
    func testSnippetMergeIsCommutativeOnATie() {
        let id = UUID()
        let a = SyncState(snippets: [snippet(id, "ett", at: 42)])
        let b = SyncState(snippets: [snippet(id, "tva", at: 42)])
        XCTAssertEqual(
            SyncEngine.merge(a, b).snippets.first?.name,
            SyncEngine.merge(b, a).snippets.first?.name)
    }

    /// Ett tillstånd skrivet INNAN snippets ingick saknar fältet helt. Det ska
    /// läsas som "inga snippets", inte avvisas — annars slutar synken fungera
    /// för alla som inte uppgraderat varje enhet samtidigt, vilket ingen gör.
    func testASyncStateWrittenBeforeSnippetsExistedStillLoads() throws {
        let json = Data(#"{"hosts":[],"tombstones":[]}"#.utf8)
        let state = try JSONDecoder().decode(SyncState.self, from: json)
        XCTAssertTrue(state.snippets.isEmpty)
        XCTAssertTrue(state.hosts.isEmpty)
    }

    /// Hela vägen genom två databaspar och en delad transport, inklusive en
    /// radering som måste hålla i sig. En radering är det som avslöjar en
    /// trasig synk: utan en gravsten som överlever hopslagningen kommer
    /// motparten glatt tillbaka med sin kopia.
    func testTwoIndependentStoresConvergeOnSnippetsToo() throws {
        let dir = NSTemporaryDirectory() + "bastion-snipsync-\(UUID().uuidString)"
        try FileManager.default.createDirectory(
            atPath: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: dir) }

        let hostsA = HostStore(path: dir + "/a.json")
        let hostsB = HostStore(path: dir + "/b.json")
        let snipsA = SnippetStore(path: dir + "/a-snippets.json")
        let snipsB = SnippetStore(path: dir + "/b-snippets.json")
        let provider = FolderSyncProvider(path: dir + "/delad.json")

        let doomed = Snippet(name: "doomed", template: "echo doomed")
        snipsA.upsert(doomed)
        snipsA.upsert(Snippet(name: "kvar", template: "echo kvar"))
        snipsB.upsert(Snippet(name: "bs-egna", template: "echo b"))

        try hostsA.sync(with: provider, snippets: snipsA)
        try hostsB.sync(with: provider, snippets: snipsB)
        try hostsA.sync(with: provider, snippets: snipsA)

        XCTAssertEqual(snipsA.all().map { $0.name }, ["bs-egna", "doomed", "kvar"])
        XCTAssertEqual(snipsB.all().map { $0.name }, snipsA.all().map { $0.name })

        // A raderar en. Raderingen måste överleva att B pushar tillbaka sin
        // kopia av samma snippet.
        snipsA.delete(doomed.id, recordingTombstoneIn: hostsA)
        try hostsA.sync(with: provider, snippets: snipsA)
        try hostsB.sync(with: provider, snippets: snipsB)
        try hostsA.sync(with: provider, snippets: snipsA)

        XCTAssertEqual(
            snipsA.all().map { $0.name }, ["bs-egna", "kvar"], "den raderade får inte återuppstå")
        XCTAssertEqual(snipsB.all().map { $0.name }, snipsA.all().map { $0.name })
    }
}
