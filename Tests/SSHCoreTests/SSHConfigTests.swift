import Foundation
import XCTest
@testable import SSHCore

final class SSHConfigTests: XCTestCase {
    // Idiomatisk OpenSSH: specifika block först, catch-all sist ("första vinner").
    let sample = """
    Host web prod-web
        HostName 10.0.0.5
        User deploy
        Port 2222
        IdentityFile ~/.ssh/deploy_ed25519

    Host *.internal
        User admin

    Host bastion-*
        ProxyJump jump.example.com

    Host !secret *
        User fallback
    """

    func testExactAliasWithAllFields() {
        let r = SSHConfig(text: sample).resolve("web")
        XCTAssertEqual(r.hostName, "10.0.0.5")
        XCTAssertEqual(r.user, "deploy")
        XCTAssertEqual(r.port, 2222)
        XCTAssertEqual(r.identityFile, (("~/.ssh/deploy_ed25519") as NSString).expandingTildeInPath)
    }

    func testSecondPatternOnSameHostLine() {
        XCTAssertEqual(SSHConfig(text: sample).resolve("prod-web").hostName, "10.0.0.5")
    }

    func testWildcardSuffix() {
        let r = SSHConfig(text: sample).resolve("db1.internal")
        XCTAssertEqual(r.user, "admin")          // *.internal matchar först
        XCTAssertEqual(r.hostName, "db1.internal") // ingen HostName -> aliaset
    }

    func testWildcardPrefixAndProxyJump() {
        XCTAssertEqual(SSHConfig(text: sample).resolve("bastion-eu").proxyJump, "jump.example.com")
    }

    func testFirstValueWins() {
        // "web" matchar både sitt eget block (User deploy) och "!secret *" (User fallback).
        // Första vinner.
        XCTAssertEqual(SSHConfig(text: sample).resolve("web").user, "deploy")
    }

    func testNegationExcludes() {
        // "secret" exkluderas av "!secret *" -> matchar inget block -> ingen User.
        let r = SSHConfig(text: sample).resolve("secret")
        XCTAssertNil(r.user)
        XCTAssertEqual(r.hostName, "secret")
    }

    func testUnknownAliasHitsCatchAll() {
        let r = SSHConfig(text: sample).resolve("random")
        XCTAssertEqual(r.hostName, "random")
        XCTAssertEqual(r.user, "fallback")   // "!secret *" catch-all
        XCTAssertEqual(r.port, 22)
    }

    func testEqualsAndSpacedSyntax() {
        let cfg = SSHConfig(text: "Host x\n  HostName=1.2.3.4\n  Port = 2200")
        let r = cfg.resolve("x")
        XCTAssertEqual(r.hostName, "1.2.3.4")
        XCTAssertEqual(r.port, 2200)
    }

    func testGlob() {
        XCTAssertTrue(SSHConfig.glob("*.internal", "a.internal"))
        XCTAssertTrue(SSHConfig.glob("bastion-*", "bastion-eu-1"))
        XCTAssertTrue(SSHConfig.glob("h??t", "host"))
        XCTAssertFalse(SSHConfig.glob("h??t", "hot"))
        XCTAssertFalse(SSHConfig.glob("*.internal", "internal"))
        XCTAssertTrue(SSHConfig.glob("*", "anything"))
    }

    // MARK: - Include

    private func makeConfigDir() -> String {
        let dir = NSTemporaryDirectory() + "bastion-include-\(UUID().uuidString)"
        try! FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
        return dir
    }

    private func write(_ text: String, to path: String) {
        try! text.write(toFile: path, atomically: true, encoding: .utf8)
    }

    /// Utan `Include` läses NOLL värdar ur en modern config, utan att något
    /// ser trasigt ut. Testet speglar exakt det upplägg 1Password och OrbStack
    /// instruerar användaren att skapa: en huvudfil som bara pekar vidare, och
    /// inte en enda `Host`-rad i den.
    func testHostsBehindAnIncludeAreFoundAndWouldNotBeWithoutIt() {
        let dir = makeConfigDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        try! FileManager.default.createDirectory(
            atPath: dir + "/config.d", withIntermediateDirectories: true)
        write("Include config.d/work\n", to: dir + "/config")
        write(
            "Host kund\n  HostName kund.example\n  User anders\n  Port 2222\n",
            to: dir + "/config.d/work")

        let config = SSHConfig.load(path: dir + "/config")
        XCTAssertEqual(config.hostAliases, ["kund"])
        let resolved = config.resolve("kund")
        XCTAssertEqual(resolved.hostName, "kund.example")
        XCTAssertEqual(resolved.user, "anders")
        XCTAssertEqual(resolved.port, 2222)

        // Kontrollen: samma text utan filsystemet ger ingenting. Utan den här
        // raden bevisar testet inte att det var Include som gjorde jobbet.
        let text = try! String(contentsOfFile: dir + "/config", encoding: .utf8)
        XCTAssertTrue(
            SSHConfig(text: text).hostAliases.isEmpty,
            "utan Include-upplösning finns ingen värd i huvudfilen — det är felet som fixas")
    }

    /// `Include ~/.ssh/config.d/*` är det vanligaste sättet raden skrivs.
    /// Ordningen måste vara sorterad: första värdet vinner per nyckel, så en
    /// ostabil läsordning skulle ge olika resultat mellan körningar på samma
    /// filer.
    func testAWildcardIncludeReadsEveryMatchInSortedOrder() {
        let dir = makeConfigDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        try! FileManager.default.createDirectory(
            atPath: dir + "/config.d", withIntermediateDirectories: true)
        write("Include config.d/*\n", to: dir + "/config")
        write("Host alfa\n  User a\n", to: dir + "/config.d/10-a")
        write("Host beta\n  User b\n", to: dir + "/config.d/20-b")
        write("Host gamma\n  User c\n", to: dir + "/config.d/30-c")

        XCTAssertEqual(SSHConfig.load(path: dir + "/config").hostAliases, ["alfa", "beta", "gamma"])
    }

    /// En config som inkluderar sig själv ska ge en trunkerad läsning, inte
    /// hänga sig. Utan djupgränsen är det här inte ett långsamt test utan ett
    /// test som aldrig återvänder.
    func testASelfIncludingConfigStopsInsteadOfLoopingForever() {
        let dir = makeConfigDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        write("Include config\nHost slut\n  User a\n", to: dir + "/config")

        XCTAssertTrue(SSHConfig.load(path: dir + "/config").hostAliases.contains("slut"))
    }

    /// En Include som pekar på en fil som inte finns får inte ta med sig
    /// resten av configen i fallet. Ett avinstallerat verktyg lämnar precis
    /// en sådan rad efter sig.
    func testAMissingIncludeTargetDoesNotDiscardTheRestOfTheConfig() {
        let dir = makeConfigDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        write(
            "Include config.d/finns-inte\nHost kvar\n  HostName kvar.example\n  User a\n",
            to: dir + "/config")

        XCTAssertEqual(SSHConfig.load(path: dir + "/config").hostAliases, ["kvar"])
    }

    /// Poster ur en inkluderad fil måste hamna på include-radens PLATS, inte
    /// sist. Annars ändras vilket värde som vinner — OpenSSH tar det första,
    /// så en omflyttning byter tyst ut användarens inställningar.
    func testIncludedSettingsLandWhereTheIncludeLineStood() {
        let dir = makeConfigDir()
        defer { try? FileManager.default.removeItem(atPath: dir) }
        write("Host server\n  User fran-huvudfilen\nInclude senare\n", to: dir + "/config")
        write("Host server\n  User fran-included\n", to: dir + "/senare")

        XCTAssertEqual(
            SSHConfig.load(path: dir + "/config").resolve("server").user, "fran-huvudfilen",
            "första värdet vinner — den inkluderade filen stod EFTER och ska inte skriva över")
    }

    // MARK: - Match

    /// `Match host` är den enda formen som går att avgöra utan en pågående
    /// anslutning, och den beter sig som `Host` — samma jokertecken, samma
    /// negation. Tidigare ignorerades hela blocket, så inställningarna i det
    /// försvann tyst.
    func testMatchHostAppliesItsSettingsJustLikeAHostBlock() {
        let config = SSHConfig(
            text: "Match host *.internal\n  User admin\n  Port 2200\n\nHost *\n  User fallback\n")
        let inner = config.resolve("db.internal")
        XCTAssertEqual(inner.user, "admin")
        XCTAssertEqual(inner.port, 2200)

        let outer = config.resolve("db.example.com")
        XCTAssertEqual(outer.user, "fallback", "blocket ska inte gälla utanför mönstret")
        XCTAssertEqual(outer.port, 22)
    }

    /// `Match all` gäller alltid — men bara EFTER sin egen rad, så det
    /// fungerar som en catch-all i slutet av filen.
    func testMatchAllAppliesToEveryAlias() {
        XCTAssertEqual(SSHConfig(text: "Match all\n  User alla\n").resolve("vadsomhelst").user, "alla")
    }

    /// Kärnan i avgränsningen. Ett kriterium vi inte kan avgöra måste göra
    /// blocket INAKTIVT, aldrig aktivt — ett felaktigt aktiverat block byter
    /// tyst ut användarens värdnamn eller nyckel mot någon annans, medan ett
    /// felaktigt överhoppat block bara ger samma resultat som innan `Match`
    /// stöddes.
    ///
    /// `exec` är det viktigaste fallet: att köra ett godtyckligt skalkommando
    /// för att avgöra en konfigurationsrad är inte en funktion som saknas.
    func testCriteriaWeCannotEvaluateLeaveTheBlockInactive() {
        for criteria in [
            "exec \"test -f /tmp/x\"", "user root", "originalhost jump",
            "localuser anders", "final", "canonical", "tagged arbete",
        ] {
            let config = SSHConfig(
                text: "Match \(criteria)\n  User skulle-inte-synas\n\nHost *\n  User riktig\n")
            XCTAssertEqual(
                config.resolve("nagon-vard").user, "riktig",
                "kriteriet \(criteria) går inte att avgöra och blocket ska då inte gälla")
        }
    }

    /// Alla kriterier på raden måste hålla, precis som i OpenSSH. Står ett
    /// avgörbart och ett oavgörbart kriterium tillsammans räcker det inte att
    /// det första stämmer.
    func testEveryCriterionOnTheLineMustHoldNotJustTheFirst() {
        let config = SSHConfig(
            text: "Match host server user root\n  Port 9999\n\nHost *\n  User a\n")
        XCTAssertEqual(
            config.resolve("server").port, 22,
            "host stämmer men user går inte att avgöra — blocket gäller inte")
    }

    /// Negation fungerar i `Match host` precis som i `Host`. Det här testet
    /// fångade en riktig bugg i Rust-porten: kriterieraden delades på komma,
    /// så mönsterlistans andra element blev ett eget okänt kriterium.
    func testMatchHostSupportsNegation() {
        let config = SSHConfig(
            text: "Match host *.internal,!secret.internal\n  User admin\n\nHost *\n  User a\n")
        XCTAssertEqual(config.resolve("db.internal").user, "admin")
        XCTAssertEqual(
            config.resolve("secret.internal").user, "a", "negationen ska undanta värden")
    }

    /// En `Match`-rad utan kriterier är trasig och ska inte aktivera något.
    /// `host` utan mönster likaså.
    func testAMatchLineWithoutUsableCriteriaNeverActivates() {
        for line in ["Match\n", "Match host\n"] {
            let config = SSHConfig(text: line + "  User skulle-inte-synas\n\nHost *\n  User riktig\n")
            XCTAssertEqual(config.resolve("x").user, "riktig", "rad: \(line)")
        }
    }

    /// `Match`-block får aldrig bidra med värdalias till importen. De
    /// beskriver villkor, inte värdar — ett alias därifrån skulle skapa en
    /// post för något som inte är en server.
    func testMatchBlocksContributeNoHostAliases() {
        let config = SSHConfig(text: "Match host produktion\n  User a\n\nHost riktig\n  User b\n")
        XCTAssertEqual(config.hostAliases, ["riktig"])
    }
}
