import Foundation
import XCTest
@testable import SSHCore

private typealias Host = SSHCore.Host   // undvik krock med Foundation.Host

final class HostStoreTests: XCTestCase {
    func testUpsertGetDeleteSorted() {
        let store = HostStore(path: nil)
        let web = Host(alias: "web", hostName: "10.0.0.5", user: "deploy", tags: ["prod"])
        let nas = Host(alias: "NAS", hostName: "10.0.0.2", user: "root", tags: ["homelab"])
        store.upsert(web)
        store.upsert(nas)

        XCTAssertEqual(store.all().map { $0.alias }, ["NAS", "web"])  // skiftlägesokänslig sort
        XCTAssertEqual(store.get(web.id)?.hostName, "10.0.0.5")

        var edited = web
        edited.port = 2222
        store.upsert(edited)
        XCTAssertEqual(store.get(web.id)?.port, 2222)
        XCTAssertEqual(store.all().count, 2)  // upsert på samma id ersätter, inte dubblerar

        store.delete(nas.id)
        XCTAssertEqual(store.all().map { $0.alias }, ["web"])
    }

    func testTagFiltering() {
        let store = HostStore(path: nil)
        store.upsert(Host(alias: "a", hostName: "h1", user: "u", tags: ["prod", "web"]))
        store.upsert(Host(alias: "b", hostName: "h2", user: "u", tags: ["homelab"]))
        store.upsert(Host(alias: "c", hostName: "h3", user: "u", tags: ["prod"]))

        XCTAssertEqual(store.hosts(withTag: "prod").map { $0.alias }, ["a", "c"])
        XCTAssertEqual(store.allTags(), ["homelab", "prod", "web"])
    }

    func testPersistAcrossInstances() throws {
        let dir = NSTemporaryDirectory() + "bastion-hosts-\(ProcessInfo.processInfo.processIdentifier)"
        let path = dir + "/hosts.json"
        defer { try? FileManager.default.removeItem(atPath: dir) }

        let h = Host(alias: "srv", hostName: "1.2.3.4", user: "admin", port: 2200,
                     tags: ["x"], auth: .keyFile("/home/u/.ssh/k"))
        var stored: Host?
        do {
            let s1 = HostStore(path: path)
            s1.upsert(h)
            stored = s1.get(h.id)   // upsert stämplar om modifiedAt
        }
        let s2 = HostStore(path: path)
        let loaded = s2.get(h.id)
        XCTAssertEqual(loaded, stored)                // full round-trip inkl. auth + tidsstämpel
        XCTAssertEqual(loaded?.target.port, 2200)
    }

    func testFavoriteAndColorTagRoundTrip() throws {
        var h = Host(alias: "prod-db", hostName: "10.0.0.9", user: "admin")
        h.isFavorite = true
        h.colorTag = "red"
        let data = try JSONEncoder().encode(h)
        let decoded = try JSONDecoder().decode(Host.self, from: data)
        XCTAssertEqual(decoded.isFavorite, true)
        XCTAssertEqual(decoded.colorTag, "red")
    }

    /// Gammal host.json (sparad innan isFavorite/colorTag fanns) ska fortfarande
    /// gå att läsa in — nycklarna saknas helt, avkodningen faller tillbaka på
    /// stored-property-defaults (false/nil) istället för att kasta.
    func testDecodesOldHostWithoutFavoriteOrColorFields() throws {
        let h = Host(alias: "legacy", hostName: "10.0.0.1", user: "root")
        var obj = try JSONSerialization.jsonObject(with: JSONEncoder().encode(h)) as! [String: Any]
        obj.removeValue(forKey: "isFavorite")
        obj.removeValue(forKey: "colorTag")
        let oldStyleData = try JSONSerialization.data(withJSONObject: obj)

        let decoded = try JSONDecoder().decode(Host.self, from: oldStyleData)
        XCTAssertEqual(decoded.isFavorite, false)
        XCTAssertNil(decoded.colorTag)
        XCTAssertEqual(decoded.alias, "legacy")
    }

    func testPlatformRoundTrip() throws {
        var h = Host(alias: "win-vps", hostName: "10.0.0.9", user: "Administrator")
        h.platform = .windowsAdmin
        let data = try JSONEncoder().encode(h)
        let decoded = try JSONDecoder().decode(Host.self, from: data)
        XCTAssertEqual(decoded.platform, .windowsAdmin)
    }

    /// Samma bakåtkompatibilitetsresonemang som favorit/färg-testet ovan —
    /// `platform` tillkom ännu senare, så en host.json från innan DET fältet
    /// fanns (men EFTER isFavorite/colorTag) måste också gå att läsa.
    func testDecodesOldHostWithoutPlatformField() throws {
        let h = Host(alias: "legacy2", hostName: "10.0.0.1", user: "root")
        var obj = try JSONSerialization.jsonObject(with: JSONEncoder().encode(h)) as! [String: Any]
        obj.removeValue(forKey: "platform")
        let oldStyleData = try JSONSerialization.data(withJSONObject: obj)

        let decoded = try JSONDecoder().decode(Host.self, from: oldStyleData)
        XCTAssertEqual(decoded.platform, .posix)
    }

    func testStartupCommandRoundTrip() throws {
        var h = Host(alias: "web", hostName: "10.0.0.9", user: "deploy")
        h.startupCommand = "cd /srv/app && tmux attach || tmux new"
        let data = try JSONEncoder().encode(h)
        let decoded = try JSONDecoder().decode(Host.self, from: data)
        XCTAssertEqual(decoded.startupCommand, "cd /srv/app && tmux attach || tmux new")
    }

    /// Samma bakåtkompatibilitetsresonemang som ovan — `startupCommand`
    /// tillkom ännu senare, så en host.json från innan DET fältet fanns
    /// måste också gå att läsa.
    func testDecodesOldHostWithoutStartupCommandField() throws {
        let h = Host(alias: "legacy3", hostName: "10.0.0.1", user: "root")
        var obj = try JSONSerialization.jsonObject(with: JSONEncoder().encode(h)) as! [String: Any]
        obj.removeValue(forKey: "startupCommand")
        let oldStyleData = try JSONSerialization.data(withJSONObject: obj)

        let decoded = try JSONDecoder().decode(Host.self, from: oldStyleData)
        XCTAssertNil(decoded.startupCommand)
    }

    func testJumpHostIDRoundTrip() throws {
        let jump = Host(alias: "bastion-host", hostName: "10.0.0.1", user: "jump")
        var target = Host(alias: "internal-db", hostName: "10.0.1.5", user: "admin")
        target.jumpHostID = jump.id
        let data = try JSONEncoder().encode(target)
        let decoded = try JSONDecoder().decode(Host.self, from: data)
        XCTAssertEqual(decoded.jumpHostID, jump.id)
    }

    /// Samma bakåtkompatibilitetsresonemang som ovan — `jumpHostID` tillkom
    /// ännu senare, så en host.json från innan DET fältet fanns måste också
    /// gå att läsa (utan jump host, precis som innan fältet fanns).
    func testDecodesOldHostWithoutJumpHostIDField() throws {
        let h = Host(alias: "legacy4", hostName: "10.0.0.1", user: "root")
        var obj = try JSONSerialization.jsonObject(with: JSONEncoder().encode(h)) as! [String: Any]
        obj.removeValue(forKey: "jumpHostID")
        let oldStyleData = try JSONSerialization.data(withJSONObject: obj)

        let decoded = try JSONDecoder().decode(Host.self, from: oldStyleData)
        XCTAssertNil(decoded.jumpHostID)
    }

    func testMacAddressRoundTrip() throws {
        var host = Host(alias: "homelab", hostName: "10.0.0.9", user: "root")
        host.macAddress = "AA:BB:CC:DD:EE:FF"
        let data = try JSONEncoder().encode(host)
        let decoded = try JSONDecoder().decode(Host.self, from: data)
        XCTAssertEqual(decoded.macAddress, "AA:BB:CC:DD:EE:FF")
    }

    /// Samma bakåtkompatibilitetsresonemang som `jumpHostID` — `macAddress`
    /// tillkom ännu senare, en host.json från innan måste gå att läsa
    /// (ingen MAC-adress, precis som innan fältet fanns).
    func testDecodesOldHostWithoutMacAddressField() throws {
        let h = Host(alias: "legacy5", hostName: "10.0.0.1", user: "root")
        var obj = try JSONSerialization.jsonObject(with: JSONEncoder().encode(h)) as! [String: Any]
        obj.removeValue(forKey: "macAddress")
        let oldStyleData = try JSONSerialization.data(withJSONObject: obj)

        let decoded = try JSONDecoder().decode(Host.self, from: oldStyleData)
        XCTAssertNil(decoded.macAddress)
    }

    // MARK: - forwardAgent

    /// Buggen det här testet finns för: fältet saknades helt i den här
    /// modellen medan LinuxApp hade det. `Codable` släpper okända nycklar
    /// tyst, så en värd med agentvidarebefordran påslagen FÖRLORADE
    /// inställningen så fort tillståndet passerade en Apple-enhet vid synk.
    /// Avkodningen kastade nyckeln, kodningen skrev inte tillbaka den, och
    /// ingenting syntes förrän nästa anslutning betedde sig annorlunda.
    func testForwardAgentSurvivesARoundTrip() throws {
        var h = Host(alias: "hopp", hostName: "10.0.0.9", user: "deploy")
        h.forwardAgent = true
        let decoded = try JSONDecoder().decode(
            Host.self, from: try JSONEncoder().encode(h))
        XCTAssertTrue(decoded.forwardAgent)

        // Nyckeln måste faktiskt FINNAS i JSON:en. Utan den här kontrollen
        // skulle testet passera även om kodningen tappade fältet och
        // avkodningen råkade defaulta till samma värde.
        let obj = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(h)) as! [String: Any]
        XCTAssertEqual(obj["forwardAgent"] as? Bool, true,
                       "fältet måste skrivas ut, annars raderas det vid nästa synk")
    }

    /// En host.json från innan fältet fanns ska fortfarande gå att läsa, och
    /// landa på det säkra värdet.
    func testDecodesOldHostWithoutForwardAgentField() throws {
        let h = Host(alias: "legacy-fa", hostName: "10.0.0.1", user: "root")
        var obj = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(h)) as! [String: Any]
        obj.removeValue(forKey: "forwardAgent")
        let decoded = try JSONDecoder().decode(
            Host.self, from: try JSONSerialization.data(withJSONObject: obj))
        XCTAssertFalse(decoded.forwardAgent, "frånvaro ska ge av, inte på")
    }

    // MARK: - Import av fält som tidigare slängdes

    /// `ForwardAgent`, `RemoteCommand` och `ProxyJump` fanns i configen men
    /// nådde aldrig fram till värden.
    func testImportCarriesForwardAgentAndRemoteCommand() {
        let store = HostStore(path: nil)
        let imported = store.importSSHConfig("""
        Host m
            HostName m.example
            User anders
            ForwardAgent yes
            RemoteCommand tmux attach
        """)
        XCTAssertEqual(imported.count, 1)
        XCTAssertTrue(imported[0].forwardAgent)
        XCTAssertEqual(imported[0].startupCommand, "tmux attach")
    }

    /// `ForwardAgent` är ett säkerhetsval, så bara ett uttryckligt ja räknas.
    func testForwardAgentRequiresAnExplicitYesOnImport() {
        let store = HostStore(path: nil)
        let imported = store.importSSHConfig("""
        Host nej
            HostName a.example
            User a
            ForwardAgent no
        Host skrap
            HostName b.example
            User a
            ForwardAgent kanske
        Host inget
            HostName c.example
            User a
        """)
        XCTAssertEqual(imported.count, 3)
        XCTAssertTrue(imported.allSatisfy { !$0.forwardAgent },
                      "varken no, skräp eller frånvaro får slå på vidarebefordran")
    }

    /// Hela vägen: en config med ProxyJump ska ge en värd som faktiskt PEKAR
    /// på jump-hosten. Utan kopplingen fungerar anslutningen inte alls,
    /// eftersom målet bara är nåbart genom hoppet.
    ///
    /// Jump-hosten står medvetet EFTER den som pekar på den — det är hela
    /// skälet till att kopplingen sker i ett andra pass.
    func testImportLinksProxyJumpToTheActualJumpHost() {
        let store = HostStore(path: nil)
        let imported = store.importSSHConfig("""
        Host inre
            HostName 10.0.0.9
            User a
            ProxyJump anders@hopp:2222
        Host hopp
            HostName hopp.example
            User anders
        """)
        let inre = imported.first { $0.alias == "inre" }
        let hopp = imported.first { $0.alias == "hopp" }
        XCTAssertNotNil(hopp)
        XCTAssertEqual(inre?.jumpHostID, hopp?.id)
        XCTAssertNil(hopp?.jumpHostID, "jump-hosten själv har inget hopp")
        // Och det ska ha SPARATS, inte bara returnerats.
        XCTAssertEqual(store.get(inre!.id)?.jumpHostID, hopp?.id)
    }

    /// Pekar ProxyJump på något som inte importerades ska värden ändå sparas,
    /// bara utan koppling. En värd som anger sig själv länkas inte heller.
    func testAnUnresolvableOrSelfReferentialProxyJumpStillImports() {
        let store = HostStore(path: nil)
        let imported = store.importSSHConfig("""
        Host inre
            HostName 10.0.0.9
            User a
            ProxyJump finns-inte
        Host sig-sjalv
            HostName x.example
            User a
            ProxyJump sig-sjalv
        """)
        XCTAssertEqual(imported.count, 2)
        XCTAssertTrue(imported.allSatisfy { $0.jumpHostID == nil })
    }

    /// Formerna `ProxyJump` skrivs i.
    func testProxyJumpAliasIsExtractedFromEveryWrittenForm() {
        XCTAssertEqual(SSHConfig.proxyJumpAlias("bastion"), "bastion")
        XCTAssertEqual(SSHConfig.proxyJumpAlias("anders@bastion"), "bastion")
        XCTAssertEqual(SSHConfig.proxyJumpAlias("anders@bastion:2222"), "bastion")
        XCTAssertEqual(SSHConfig.proxyJumpAlias("bastion,inre"), "bastion", "bara första hoppet")
        XCTAssertEqual(SSHConfig.proxyJumpAlias("[::1]:22"), "[::1]", "IPv6 får inte klippas vid kolon")
        XCTAssertNil(SSHConfig.proxyJumpAlias("none"))
        XCTAssertNil(SSHConfig.proxyJumpAlias(""))
    }

    // MARK: - Trådformatet mot de andra plattformarna

    /// Låser HELA JSON-formen för `Host`, inte bara ett fält.
    ///
    /// Två fältnamn har redan tyst gått isär mellan den här sidan och
    /// LinuxApp: `forwardAgent` saknades helt här, och `jumpHostID` stavades
    /// `jumpHostId` där. Båda gångerna var symptomet dataförlust vid synk
    /// utan ett enda felmeddelande. Ett test som bara kollar de fält man
    /// råkar tänka på hittar inte nästa — det här kräver att
    /// nyckeluppsättningen är EXAKT den förväntade, så att den som lägger
    /// till ett fält tvingas svara på om Rust-sidan stavar det likadant.
    ///
    /// Värden fylls i helt och hållet: `encodeIfPresent` utelämnar nil, så en
    /// halvfylld värd skulle dölja precis de valfria fälten som är lättast
    /// att stava fel.
    func testTheJSONShapeOfHostIsExactlyWhatTheOtherPlatformsExpect() throws {
        var h = Host(alias: "a", hostName: "h", user: "u")
        h.colorTag = "blue"
        h.startupCommand = "tmux"
        h.jumpHostID = UUID()
        h.macAddress = "AA:BB:CC:DD:EE:FF"
        h.forwardAgent = true

        let obj = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(h)) as! [String: Any]
        XCTAssertEqual(
            obj.keys.sorted(),
            [
                "alias", "auth", "colorTag", "forwardAgent", "hostName", "id", "isFavorite",
                "jumpHostID", "macAddress", "modifiedAt", "platform", "port", "startupCommand",
                "tags", "user",
            ].sorted(),
            "JSON-formen ändrades. Uppdatera LinuxApps serde-attribut i samma veva, annars "
                + "släpps det nya fältet tyst vid synk.")
    }

    /// Den gamla stavningen fanns bara i filer skrivna av LinuxApp. Den ska
    /// LÄSAS så att ingen tappar sin ProxyJump-koppling vid uppgraderingen —
    /// men aldrig skrivas, för då skulle nästa läsare få två källor till
    /// samma sanning.
    func testTheOldJumpHostSpellingIsReadButNeverWritten() throws {
        let jumpID = UUID()
        var obj = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(Host(alias: "a", hostName: "h", user: "u")))
            as! [String: Any]
        obj["jumpHostId"] = jumpID.uuidString   // gammal stavning, som LinuxApp skrev
        let decoded = try JSONDecoder().decode(
            Host.self, from: try JSONSerialization.data(withJSONObject: obj))
        XCTAssertEqual(decoded.jumpHostID, jumpID, "den gamla stavningen måste fortfarande läsas")

        let reencoded = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(decoded)) as! [String: Any]
        XCTAssertNil(reencoded["jumpHostId"], "den gamla stavningen får aldrig skrivas tillbaka")
        XCTAssertEqual(reencoded["jumpHostID"] as? String, jumpID.uuidString)
    }

    /// Står båda stavningarna i samma fil vinner den nya. Kan bara inträffa
    /// i en fil skriven under övergången, men tystnad är fel svar på
    /// tvetydighet.
    func testTheNewSpellingWinsWhenBothArePresent() throws {
        let newID = UUID()
        var h = Host(alias: "a", hostName: "h", user: "u")
        h.jumpHostID = newID
        var obj = try JSONSerialization.jsonObject(
            with: try JSONEncoder().encode(h)) as! [String: Any]
        obj["jumpHostId"] = UUID().uuidString
        let decoded = try JSONDecoder().decode(
            Host.self, from: try JSONSerialization.data(withJSONObject: obj))
        XCTAssertEqual(decoded.jumpHostID, newID)
    }

    /// Guldfixtur som LinuxApp avkodar från SAMMA fil, i
    /// `host::tests::the_shared_wire_format_fixture_decodes_to_the_expected_host`.
    ///
    /// Nyckeljämförelsen ovan fångar att ett fält HETER samma sak. Den säger
    /// ingenting om nyttolastens FORM — att `HostAuth` bär sina fält som
    /// `{"certificateFile": {"keyPath": …}}` och att `platform` är en rå
    /// sträng, inte ett objekt. Går den formen isär tappas fältet lika tyst
    /// som ett felstavat nyckelnamn.
    ///
    /// Ändra aldrig fixturen för att få testet grönt. Faller den har
    /// trådformatet ändrats, och då är frågan vad som händer med redan
    /// sparade filer hos användarna.
    func testTheSharedWireFormatFixtureDecodesIdentically() throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()      // SSHCoreTests
            .deletingLastPathComponent()      // Tests
            .appendingPathComponent("fixtures/host-wire-format.json")
        let host = try JSONDecoder().decode(Host.self, from: Data(contentsOf: fixture))

        XCTAssertEqual(host.id, UUID(uuidString: "6f1c9b2e-3a4d-4f57-8b91-0c2d5e7a1234"))
        XCTAssertEqual(host.alias, "guld")
        XCTAssertEqual(host.hostName, "10.0.0.7")
        XCTAssertEqual(host.user, "anders")
        XCTAssertEqual(host.port, 2222)
        XCTAssertEqual(host.tags, ["prod", "eu"])
        XCTAssertEqual(
            host.auth,
            .certificateFile(
                keyPath: "/home/anders/.ssh/id_ed25519",
                certPath: "/home/anders/.ssh/id_ed25519-cert.pub"),
            "HostAuth-nyttolasten måste ha samma form som LinuxApps handskrivna serde-kod")
        XCTAssertTrue(host.isFavorite)
        XCTAssertEqual(host.colorTag, "blue")
        XCTAssertEqual(host.platform, .windowsAdmin)
        XCTAssertEqual(host.startupCommand, "tmux attach")
        XCTAssertEqual(host.jumpHostID, UUID(uuidString: "b7e4d1a0-8c33-4e29-9f6b-1d5a3c8e9876"))
        XCTAssertEqual(host.macAddress, "AA:BB:CC:DD:EE:FF")
        XCTAssertTrue(host.forwardAgent)
        XCTAssertEqual(host.modifiedAt.timeIntervalSinceReferenceDate, 800_000_000, accuracy: 0.001)

        // Och tillbaka: det vi skriver ska gå att läsa som samma sak igen.
        let roundTripped = try JSONDecoder().decode(
            Host.self, from: try JSONEncoder().encode(host))
        XCTAssertEqual(roundTripped.jumpHostID, host.jumpHostID)
        XCTAssertEqual(roundTripped.auth, host.auth)
        XCTAssertEqual(roundTripped.forwardAgent, host.forwardAgent)
    }
}
