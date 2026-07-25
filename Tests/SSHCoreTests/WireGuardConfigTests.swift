import XCTest
@testable import SSHCore

final class WireGuardConfigTests: XCTestCase {
    private static let testPrivateKey = ProcessInfo.processInfo.environment["BASTION_TEST_WIREGUARD_PRIVATE_KEY"]
        ?? "safe-test-private-key"
    private static let testPresharedKey = ProcessInfo.processInfo.environment["BASTION_TEST_WIREGUARD_PRESHARED_KEY"]
        ?? "safe-test-preshared-key"

    /// Realistisk exempelkonfiguration i det dokumenterade formatet
    /// (wg(8)/wg-quick(8), man7.org) — kommentarer, flera peers, blandad
    /// skiftlägesanvändning på nycklarna (verkliga filer varierar).
    private let sample = """
    # min hem-VPN
    [Interface]
    PrivateKey = \(WireGuardConfigTests.testPrivateKey)
    Address = 10.0.0.2/24, fd00::2/64
    DNS = 1.1.1.1, home.example.com
    ListenPort = 51820
    MTU = 1420
    Table = auto
    PostUp = iptables -A FORWARD -i %i -j ACCEPT
    PostDown = iptables -D FORWARD -i %i -j ACCEPT
    SaveConfig = true

    [Peer]
    # servern hemma
    PublicKey = HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=
    PresharedKey = \(WireGuardConfigTests.testPresharedKey)
    AllowedIPs = 0.0.0.0/0, ::/0
    Endpoint = vpn.example.com:51820
    PersistentKeepalive = 25

    [PEER]
    PublicKey = anotherKeyBase64Placeholder1234567890abcdef=
    AllowedIPs = 10.0.0.3/32
    """

    func testParsesInterfaceFields() {
        let config = WireGuardConfig(text: sample)
        XCTAssertEqual(config.interface.privateKey, Self.testPrivateKey)
        XCTAssertEqual(config.interface.address, ["10.0.0.2/24", "fd00::2/64"])
        XCTAssertEqual(config.interface.dns, ["1.1.1.1", "home.example.com"])
        XCTAssertEqual(config.interface.listenPort, 51820)
        XCTAssertEqual(config.interface.mtu, 1420)
        XCTAssertEqual(config.interface.table, "auto")
        XCTAssertEqual(config.interface.postUp, ["iptables -A FORWARD -i %i -j ACCEPT"])
        XCTAssertEqual(config.interface.postDown, ["iptables -D FORWARD -i %i -j ACCEPT"])
        XCTAssertEqual(config.interface.saveConfig, true)
    }

    func testParsesMultiplePeersIncludingCaseInsensitiveSectionHeader() {
        let config = WireGuardConfig(text: sample)
        XCTAssertEqual(config.peers.count, 2)
        XCTAssertEqual(config.peers[0].publicKey, "HIgo9xNzJMWLKASShiTqIybxZ0U3wGLiUeJ1PKf8ykw=")
        XCTAssertEqual(config.peers[0].presharedKey, Self.testPresharedKey)
        XCTAssertEqual(config.peers[0].allowedIPs, ["0.0.0.0/0", "::/0"])
        XCTAssertEqual(config.peers[0].endpoint, "vpn.example.com:51820")
        XCTAssertEqual(config.peers[0].persistentKeepalive, 25)
        // "[PEER]" (versaler) ska tolkas likadant som "[Peer]".
        XCTAssertEqual(config.peers[1].publicKey, "anotherKeyBase64Placeholder1234567890abcdef=")
        XCTAssertEqual(config.peers[1].allowedIPs, ["10.0.0.3/32"])
    }

    func testCommentsAreStripped() {
        let config = WireGuardConfig(text: sample)
        // Kommentarraden "# servern hemma" ska inte påverka parsningen —
        // redan implicit bevisat av de andra testerna, men ett explicit
        // test för en kommentar EFTER ett värde på samma rad:
        let withInlineComment = "[Interface]\nPrivateKey = abc123= # min nyckel\n"
        let parsed = WireGuardConfig(text: withInlineComment)
        XCTAssertEqual(parsed.interface.privateKey, "abc123=")
    }

    /// CodeRabbit-fynd, PR #79: en rad som BÖRJAR med "#" (en helt
    /// utkommenterad nyckel, t.ex. från en användare som tillfälligt
    /// stängt av en Address-rad) fick tidigare sin ledande tomma sträng
    /// tappad av `split(separator: "#", maxSplits: 1)` (default
    /// `omittingEmptySubsequences: true`) — resultatet blev att texten
    /// EFTER "#" felaktigt tolkades som aktiv config istället för att
    /// ignoreras som en kommentar.
    func testLeadingHashCommentLineIsIgnoredNotParsedAsActiveConfig() {
        let text = "[Interface]\n#Address = 10.0.0.99/32\nPrivateKey = x\n"
        let config = WireGuardConfig(text: text)
        XCTAssertTrue(config.interface.address.isEmpty)
        XCTAssertEqual(config.interface.privateKey, "x")
    }

    func testRoundTripThroughRenderedPreservesAllFields() {
        let original = WireGuardConfig(text: sample)
        let rerendered = WireGuardConfig(text: original.rendered())
        XCTAssertEqual(original, rerendered)
    }

    func testEmptyConfigRendersOnlyInterfaceHeader() {
        let config = WireGuardConfig()
        XCTAssertEqual(config.rendered(), "[Interface]\n")
    }

    func testMissingOptionalFieldsStayNil() {
        let config = WireGuardConfig(text: "[Interface]\nPrivateKey = abc=\n")
        XCTAssertNil(config.interface.listenPort)
        XCTAssertNil(config.interface.mtu)
        XCTAssertNil(config.interface.saveConfig)
        XCTAssertTrue(config.interface.address.isEmpty)
        XCTAssertTrue(config.peers.isEmpty)
    }

    func testRepeatedAddressLinesAccumulateInsteadOfOverwriting() {
        let text = """
        [Interface]
        Address = 10.0.0.2/24
        Address = fd00::2/64
        """
        let config = WireGuardConfig(text: text)
        XCTAssertEqual(config.interface.address, ["10.0.0.2/24", "fd00::2/64"])
    }

    func testSaveConfigFalse() {
        let config = WireGuardConfig(text: "[Interface]\nSaveConfig = false\n")
        XCTAssertEqual(config.interface.saveConfig, false)
    }

    func testPeerWithoutPrecedingInterfaceSectionIsIgnored() {
        // Nycklar innan någon sektionsrubrik hör inte hemma någonstans —
        // ska ignoreras tyst, inte krascha eller hamna fel.
        let text = "PrivateKey = orphan=\n[Peer]\nPublicKey = pk=\n"
        let config = WireGuardConfig(text: text)
        XCTAssertNil(config.interface.privateKey)
        XCTAssertEqual(config.peers.first?.publicKey, "pk=")
    }
}
