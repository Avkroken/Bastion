import XCTest
@testable import SSHCore

final class WireGuardProfileStoreTests: XCTestCase {
    private static let testPrivateKey = ProcessInfo.processInfo.environment["BASTION_TEST_WIREGUARD_PRIVATE_KEY"]
        ?? "safe-test-private-key"

    private func makeConfig(privateKey: String) -> WireGuardConfig {
        var config = WireGuardConfig()
        config.interface.privateKey = privateKey
        config.interface.address = ["10.0.0.2/24"]
        config.peers = [WireGuardConfig.Peer(publicKey: "pk=", allowedIPs: ["0.0.0.0/0"])]
        return config
    }

    func testUpsertGetDeleteSorted() {
        let store = WireGuardProfileStore(path: nil)
        let home = WireGuardProfile(name: "Hemma", config: makeConfig(privateKey: "a="))
        let work = WireGuardProfile(name: "jobbet", config: makeConfig(privateKey: "b="))
        store.upsert(home)
        store.upsert(work)

        XCTAssertEqual(store.all().map(\.name), ["Hemma", "jobbet"])  // skiftlägesokänslig sort
        XCTAssertEqual(store.get(home.id)?.config.interface.privateKey, "a=")

        store.delete(work.id)
        XCTAssertEqual(store.all().map(\.name), ["Hemma"])
    }

    func testPersistAcrossInstances() throws {
        let dir = NSTemporaryDirectory() + "bastion-wireguard-\(ProcessInfo.processInfo.processIdentifier)"
        let path = dir + "/wireguard.json"
        defer { try? FileManager.default.removeItem(atPath: dir) }

        let profile = WireGuardProfile(name: "Hemma", config: makeConfig(privateKey: "a="))
        var stored: WireGuardProfile?
        do {
            let s1 = WireGuardProfileStore(path: path)
            s1.upsert(profile)
            stored = s1.get(profile.id)
        }
        let s2 = WireGuardProfileStore(path: path)
        XCTAssertEqual(s2.get(profile.id), stored)
    }

    /// Bevisar hela vägen: text -> WireGuardConfig -> WireGuardProfile ->
    /// lagrad JSON -> ny store-instans -> tillbaka till .conf-text, allt
    /// identiskt med originalet.
    func testFullRoundTripThroughStoreAndBackToConfText() throws {
        let privateKey = ProcessInfo.processInfo.environment["BASTION_TEST_WIREGUARD_PRIVATE_KEY"]
            ?? "safe-test-private-key"
        let text = """
        [Interface]
        PrivateKey = \(privateKey)
        Address = 10.0.0.2/24

        [Peer]
        PublicKey = HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=
        AllowedIPs = 0.0.0.0/0
        Endpoint = vpn.example.com:51820
        """
        let config = WireGuardConfig(text: text)
        let profile = WireGuardProfile(name: "Hemma", config: config)

        let dir = NSTemporaryDirectory() + "bastion-wireguard-roundtrip-\(ProcessInfo.processInfo.processIdentifier)"
        let path = dir + "/wireguard.json"
        defer { try? FileManager.default.removeItem(atPath: dir) }
        let s1 = WireGuardProfileStore(path: path)
        s1.upsert(profile)

        let s2 = WireGuardProfileStore(path: path)
        let reloaded = s2.get(profile.id)
        XCTAssertEqual(reloaded?.config, config)
        XCTAssertEqual(reloaded?.config.rendered(), config.rendered())
    }
}
