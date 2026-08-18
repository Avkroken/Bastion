import XCTest
@testable import SSHCore

// Tester för de sex integrationerna som portats från
// LinuxApp/integrations/. Testfallen speglar Rust-sidans avsiktligt: samma
// kantfall, samma resonemang. Skiljer de två sidorna sig åt är det ett fel
// oavsett vilken som har rätt.

final class KubernetesServiceTests: XCTestCase {
    /// RFC 1123-etiketter — det API-servern faktiskt accepterar, alltså
    /// snävare än Dockers regel.
    func testValidationAcceptsOnlyRFC1123Labels() throws {
        for good in ["nginx", "my-app-7d9f", "a", "web2", "x9-y"] {
            XCTAssertNoThrow(try KubernetesService.validate(good), good)
        }
        for bad in ["Nginx", "my_app", "app.prod", "-leading", "trailing-", ""] {
            XCTAssertThrowsError(try KubernetesService.validate(bad), bad)
        }
        XCTAssertThrowsError(try KubernetesService.validate(String(repeating: "a", count: 64)))
    }

    func testInjectionCannotReachCommandBuilders() {
        for bad in ["pod; rm -rf /", "pod && curl evil", "pod$(id)", "pod|tee x", "'pod'", "a b"] {
            XCTAssertThrowsError(try KubernetesService.podLogsCommand(namespace: "default", pod: bad, tail: 100))
            XCTAssertThrowsError(try KubernetesService.deletePodCommand(namespace: "default", pod: bad))
            XCTAssertThrowsError(try KubernetesService.restartDeploymentCommand(namespace: "default", deployment: bad))
        }
    }

    /// "Alla namnrymder" och "default" är motsatser, och typen tvingar fram
    /// valet i stället för att låta nil betyda båda.
    func testNamespaceAllAndNamedProduceDifferentFlags() throws {
        XCTAssertEqual(
            try KubernetesService.podsCommand(.all),
            "kubectl get pods --all-namespaces --no-headers 2>/dev/null"
        )
        XCTAssertEqual(
            try KubernetesService.podsCommand(.named("kube-system")),
            "kubectl get pods -n kube-system --no-headers 2>/dev/null"
        )
        XCTAssertThrowsError(try KubernetesService.podsCommand(.named("Fel Namn")))
    }

    /// Kärnan i vyn: en podd i Running med 1/3 klara containrar är INTE frisk.
    func testRunningWithUnreadyContainersIsNotHealthy() {
        let out = """
        web-1  3/3  Running    0  1d
        web-2  1/3  Running    7  1d
        job-1  0/1  Completed  0  2h
        web-3  0/1  CrashLoopBackOff  12  10m
        """
        let pods = KubernetesService.parsePods(out, namespace: .named("default"))
        XCTAssertEqual(pods.count, 4)
        XCTAssertEqual(pods.filter(\.isHealthy).map(\.name), ["web-1", "job-1"])
        XCTAssertEqual(pods[1].restarts, "7")
    }

    /// Kolumnerna skiftar med --all-namespaces, så namnrymden måste följa med
    /// in i parsningen.
    func testAllNamespacesOutputHasAnExtraLeadingColumn() {
        let withNS = KubernetesService.parsePods(
            "kube-system  coredns-abc  1/1  Running  0  5d", namespace: .all
        )
        XCTAssertEqual(withNS.first?.namespace, "kube-system")
        XCTAssertEqual(withNS.first?.name, "coredns-abc")

        let without = KubernetesService.parsePods(
            "coredns-abc  1/1  Running  0  5d", namespace: .named("kube-system")
        )
        XCTAssertEqual(without.first?.namespace, "kube-system")
        XCTAssertEqual(without.first?.name, "coredns-abc")
    }

    /// En avstängd nod rapporteras som `Ready,SchedulingDisabled` — frisk OCH
    /// avstängd, och likhetsjämförelse hade missat båda.
    func testCordonedNodeIsBothReadyAndDisabled() {
        let out = """
        node-a  Ready                     control-plane  30d  v1.31.2
        node-b  Ready,SchedulingDisabled  <none>         30d  v1.31.2
        node-c  NotReady                  <none>         30d  v1.30.8
        """
        let nodes = KubernetesService.parseNodes(out)
        XCTAssertEqual(nodes.count, 3)
        XCTAssertTrue(nodes[0].isReady)
        XCTAssertFalse(nodes[0].isCordoned)
        XCTAssertTrue(nodes[1].isReady, "avstängd är inte samma sak som trasig")
        XCTAssertTrue(nodes[1].isCordoned)
        XCTAssertFalse(nodes[2].isReady)
        // Versionen tas bakifrån eftersom ROLES varierar.
        XCTAssertEqual(nodes[0].version, "v1.31.2")
        XCTAssertEqual(nodes[2].version, "v1.30.8")
    }

    func testRaggedSpacingAndJunkLinesAreHandled() {
        XCTAssertTrue(KubernetesService.parsePods("", namespace: .named("default")).isEmpty)
        XCTAssertTrue(KubernetesService.parsePods("bara-namn", namespace: .named("default")).isEmpty)
        XCTAssertTrue(KubernetesService.parseNodes("node-a  Ready").isEmpty)

        let pods = KubernetesService.parsePods(
            "\nweb-1     1/1        Running   0     1d\ntrasig\n", namespace: .named("default")
        )
        XCTAssertEqual(pods.count, 1)
        XCTAssertEqual(pods[0].name, "web-1")
    }
}

final class ProxmoxServiceTests: XCTestCase {
    func testOnlyIntegersFrom100AreValidVMIDs() {
        for good in ["100", "101", "9999", "123456789"] {
            XCTAssertNoThrow(try ProxmoxService.validateVMID(good), good)
        }
        for bad in ["", "99", "1", "0", "-100", "10a", "1234567890", "web", "100 "] {
            XCTAssertThrowsError(try ProxmoxService.validateVMID(bad), bad)
        }
    }

    func testInjectionCannotReachCommandBuilders() {
        for bad in ["100; rm -rf /", "100 && curl evil", "100$(id)", "100|tee x", "'100'"] {
            for kind in [ProxmoxGuestKind.vm, .container] {
                XCTAssertThrowsError(try ProxmoxService.startCommand(kind, bad))
                XCTAssertThrowsError(try ProxmoxService.shutdownCommand(kind, bad))
                XCTAssertThrowsError(try ProxmoxService.stopCommand(kind, bad))
            }
        }
    }

    /// Samma åtgärd, olika verktyg — hela skillnaden mellan en VM och en
    /// container här.
    func testVMsUseQmAndContainersUsePct() throws {
        XCTAssertEqual(try ProxmoxService.startCommand(.vm, "100"), "qm start 100 2>&1")
        XCTAssertEqual(try ProxmoxService.startCommand(.container, "100"), "pct start 100 2>&1")
    }

    /// `shutdown` går via gästens OS, `stop` drar ur strömmen. Att de är olika
    /// kommandon är avsiktligt.
    func testShutdownAndStopAreDifferentCommands() throws {
        let clean = try ProxmoxService.shutdownCommand(.vm, "100")
        let hard = try ProxmoxService.stopCommand(.vm, "100")
        XCTAssertNotEqual(clean, hard)
        XCTAssertTrue(clean.contains("shutdown"))
        XCTAssertTrue(hard.contains(" stop "))
    }

    /// Fällan i `pct list`: kolumnen Lock är TOM för en olåst container, så
    /// fältantalet varierar och namnet måste tas bakifrån.
    func testContainerNameIsTakenFromTheEndBecauseLockMayBeEmpty() {
        let out = """
        VMID       Status     Lock         Name
        100        running                 pihole
        101        stopped    backup       nextcloud
        """
        let cts = ProxmoxService.parseContainers(out)
        XCTAssertEqual(cts.count, 2)
        XCTAssertEqual(cts[0].name, "pihole", "olåst: tre fält, namnet sist")
        XCTAssertTrue(cts[0].isRunning)
        XCTAssertEqual(cts[1].name, "nextcloud", "låst: fyra fält, namnet ändå sist")
        XCTAssertFalse(cts[1].isRunning)
    }

    func testVMListSkipsTheHeaderAndReservedIDs() {
        let out = """
              VMID NAME                 STATUS     MEM(MB)    BOOTDISK(GB) PID
               100 web                  running    2048              32.00 1234
               101 db                   stopped    4096              64.00 0
                99 reserverad           running    512               8.00  1
        """
        let vms = ProxmoxService.parseVMs(out)
        XCTAssertEqual(vms.count, 2, "rubriken och VMID under 100 ska hoppas över")
        XCTAssertEqual(vms[0].name, "web")
        XCTAssertTrue(vms[0].isRunning)
        XCTAssertFalse(vms[1].isRunning)
    }

    func testStorageReportsTypeStatusAndUsage() {
        let out = """
        Name             Type     Status           Total            Used       Available        %
        local             dir     active        98559220        12345678        81181542   12.53%
        tank              zfs     active      3844505600      2818572288      1025933312   73.31%
        backup            nfs   inactive               0               0               0    0.00%
        """
        let s = ProxmoxService.parseStorage(out)
        XCTAssertEqual(s.count, 3)
        XCTAssertTrue(s[0].isActive)
        XCTAssertEqual(s[1].usedPercent, "73.31%")
        XCTAssertFalse(s[2].isActive)
    }
}

final class TrueNASServiceTests: XCTestCase {
    func testOnlyMiddlewareServiceIDsAreValid() {
        for good in ["cifs", "nfs", "ssh", "smartd", "iscsitarget", "s3"] {
            XCTAssertNoThrow(try TrueNASService.validateService(good), good)
        }
        for bad in ["CIFS", "nfs-server", "nfs.service", ""] {
            XCTAssertThrowsError(try TrueNASService.validateService(bad), bad)
        }
    }

    /// Argumentet är JSON, alltså en citerad sträng inuti skalcitationen. Att
    /// valideringen uteslutit apostrof gör konstruktionen säker.
    func testServiceCommandsWrapTheIDAsJSONInsideShellQuotes() throws {
        XCTAssertEqual(
            try TrueNASService.startServiceCommand("cifs"),
            "midclt call service.start '\"cifs\"' 2>&1"
        )
        for bad in ["cifs'; rm -rf /", "cifs\"", "cifs$(id)", "a b"] {
            XCTAssertThrowsError(try TrueNASService.startServiceCommand(bad), bad)
            XCTAssertThrowsError(try TrueNASService.stopServiceCommand(bad), bad)
        }
    }

    /// Kärnan i poolvyn: healthy är INTE samma sak som status == "ONLINE".
    func testAnOnlinePoolCanStillBeUnhealthy() {
        let out = """
        [{"name": "tank", "status": "ONLINE", "healthy": true},
         {"name": "backup", "status": "ONLINE", "healthy": false},
         {"name": "gammal", "status": "DEGRADED", "healthy": false}]
        """
        let pools = TrueNASService.parsePools(out)
        XCTAssertEqual(pools.count, 3)
        XCTAssertFalse(pools[0].needsAttention)
        XCTAssertTrue(pools[1].needsAttention, "ONLINE men ohälsosam ska varna")
        XCTAssertTrue(pools[2].needsAttention)
    }

    /// En äldre middleware utan `healthy` ska ge en varning, inte ett tyst
    /// godkännande.
    func testAMissingHealthyFieldCountsAsUnhealthy() {
        let pools = TrueNASService.parsePools(#"[{"name": "tank", "status": "ONLINE"}]"#)
        XCTAssertEqual(pools.count, 1)
        XCTAssertFalse(pools[0].healthy)
        XCTAssertTrue(pools[0].needsAttention)
    }

    /// Kör men startar inte vid uppstart — överlever inte en omstart.
    func testARunningServiceThatIsNotEnabledIsFlagged() {
        let out = """
        [{"service": "cifs", "state": "RUNNING", "enable": true},
         {"service": "nfs",  "state": "RUNNING", "enable": false},
         {"service": "ssh",  "state": "STOPPED", "enable": true}]
        """
        let services = TrueNASService.parseServices(out)
        XCTAssertEqual(services.count, 3)
        XCTAssertFalse(services[0].isRunningButNotEnabled)
        XCTAssertTrue(services[1].isRunningButNotEnabled)
        XCTAssertFalse(services[2].isRunningButNotEnabled, "stoppad kan inte tappa något")
    }

    func testAlertLevelsSeparateBrokenFromInformational() {
        let out = """
        [{"level": "CRITICAL", "formatted": "Pool tank is DEGRADED", "dismissed": false},
         {"level": "WARNING",  "formatted": "Ny uppdatering finns", "dismissed": false},
         {"level": "ERROR",    "formatted": "Disk sda har fel", "dismissed": true}]
        """
        let alerts = TrueNASService.parseAlerts(out)
        XCTAssertEqual(alerts.count, 3)
        XCTAssertTrue(alerts[0].isCritical)
        XCTAssertFalse(alerts[1].isCritical, "WARNING är information, inte trasigt")
        XCTAssertTrue(alerts[2].dismissed)
    }

    func testNonJSONAndEmptyOutputYieldNothing() {
        for bad in ["", "   ", "Traceback (most recent call last):", #"{"inte": "en array"}"#, "null"] {
            XCTAssertTrue(TrueNASService.parsePools(bad).isEmpty, bad)
            XCTAssertTrue(TrueNASService.parseServices(bad).isEmpty, bad)
            XCTAssertTrue(TrueNASService.parseAlerts(bad).isEmpty, bad)
        }
    }
}

final class UnraidServiceTests: XCTestCase {
    static let status = """
    sbName=/boot/config/super.dat
    mdState=STARTED
    mdNumDisks=3
    mdNumDisabled=0
    mdResync=0
    mdResyncPos=0
    diskName.0=md1
    diskSize.0=3907018532
    diskState.0=7
    diskName.1=md2
    diskSize.1=1953514552
    diskState.1=7
    diskName.2=
    diskSize.2=0
    diskState.2=0
    """

    func testArrayStatusIsReadFromKeyValuePairs() throws {
        let status = try XCTUnwrap(UnraidService.parseStatus(Self.status))
        XCTAssertTrue(status.isStarted)
        XCTAssertEqual(status.diskCount, 3)
        XCTAssertFalse(status.hasDisabledDisks)
        XCTAssertNil(status.resync, "mdResync=0 betyder ingen pågående kontroll")
    }

    /// Utan mdState är svaret inte från mdcmd. Att bygga en status av tomma
    /// fält hade gett en vy som ser fungerande ut mot fel sorts maskin.
    func testAResponseWithoutMdStateIsNotAStatus() {
        XCTAssertNil(UnraidService.parseStatus(""))
        XCTAssertNil(UnraidService.parseStatus("bash: mdcmd: command not found"))
        XCTAssertNil(UnraidService.parseStatus("sbVersion=2.9.13\nmdNumDisks=3"))
    }

    /// Tomma slottar rapporteras med `diskName.N=` och ska inte bli rader.
    func testEmptySlotsAreNotDisksAndOrderFollowsTheSlot() {
        let disks = UnraidService.parseDisks(Self.status)
        XCTAssertEqual(disks.count, 2, "den tomma slotten ska inte bli en disk")
        XCTAssertEqual(disks[0].name, "md1")
        XCTAssertEqual(disks[1].name, "md2")
        XCTAssertLessThan(disks[0].slot, disks[1].slot, "sortering på slot, inte hashordning")
        XCTAssertEqual(disks[0].sizeBlocks, 3_907_018_532)
        XCTAssertEqual(disks[0].sizeBytes, 3_907_018_532 * 1024)
    }

    /// En avstängd disk betyder att arrayen kör på paritet.
    func testADisabledDiskIsReportedEvenWhenTheArrayIsStarted() throws {
        let status = try XCTUnwrap(
            UnraidService.parseStatus("mdState=STARTED\nmdNumDisks=3\nmdNumDisabled=1\nmdResync=0")
        )
        XCTAssertTrue(status.isStarted, "arrayen kör fortfarande")
        XCTAssertTrue(status.hasDisabledDisks, "men på paritet")
    }

    /// Noll som total betyder INGEN resync, inte "noll procent klart".
    func testResyncProgressIsAbsentRatherThanZeroWhenNothingRuns() throws {
        let running = try XCTUnwrap(
            UnraidService.parseStatus("mdState=STARTED\nmdResync=1000\nmdResyncPos=250")?.resync
        )
        XCTAssertEqual(running.fraction, 0.25)
        XCTAssertNil(UnraidService.parseStatus("mdState=STARTED\nmdResync=0\nmdResyncPos=0")?.resync)
        XCTAssertEqual(UnraidResync(position: 2000, total: 1000).fraction, 1.0)
        XCTAssertNil(UnraidResync(position: 0, total: 0).fraction)
    }

    /// Ett värde kan innehålla likhetstecken — delningen sker på det FÖRSTA.
    func testValuesMayContainEqualsSigns() throws {
        let status = try XCTUnwrap(
            UnraidService.parseStatus("mdState=STARTED\nsbName=/boot/config/super.dat?a=b=c")
        )
        XCTAssertTrue(status.isStarted)
        let disks = UnraidService.parseDisks("diskName.0=md=1\ndiskSize.0=100\ndiskState.0=7")
        XCTAssertEqual(disks.first?.name, "md=1")
    }

    func testSharesAreDirectoryNamesAndBlankLinesAreSkipped() {
        XCTAssertEqual(
            UnraidService.parseShares("appdata\nisos\n\n  domains  \n"),
            ["appdata", "isos", "domains"]
        )
        XCTAssertTrue(UnraidService.parseShares("").isEmpty)
    }
}

final class CloudflareServiceTests: XCTestCase {
    static let tunnels = """
    [{"id": "6ff42ae2", "name": "homelab", "connections": [
        {"colo_name": "ARN", "is_pending_reconnect": false},
        {"colo_name": "ARN", "is_pending_reconnect": false},
        {"colo_name": "HEL", "is_pending_reconnect": false}]},
     {"id": "8a1b3c4d", "name": "gammal-tunnel", "connections": []}]
    """

    /// Kärnan i vyn: en tunnel kan FINNAS och ändå vara nere.
    func testATunnelWithoutConnectionsExistsButIsDown() {
        let tunnels = CloudflareService.parseTunnels(Self.tunnels)
        XCTAssertEqual(tunnels.count, 2)
        XCTAssertTrue(tunnels[0].isUp)
        XCTAssertFalse(tunnels[1].isUp, "noll anslutningar betyder nere")
    }

    /// En tunnel som bara väntar på återanslutning tar ingen trafik.
    func testPendingReconnectDoesNotCountAsUp() {
        let waiting = #"[{"name": "t", "connections": [{"colo_name": "ARN", "is_pending_reconnect": true}]}]"#
        XCTAssertFalse(CloudflareService.parseTunnels(waiting)[0].isUp)

        let mixed = """
        [{"name": "t", "connections": [
            {"colo_name": "ARN", "is_pending_reconnect": true},
            {"colo_name": "HEL", "is_pending_reconnect": false}]}]
        """
        XCTAssertTrue(CloudflareService.parseTunnels(mixed)[0].isUp, "en levande anslutning räcker")
    }

    /// cloudflared öppnar flera anslutningar per colo — dubbletter är brus.
    func testDuplicateColosAreCollapsed() {
        let tunnels = CloudflareService.parseTunnels(Self.tunnels)
        XCTAssertEqual(tunnels[0].colos, ["ARN", "HEL"])
        XCTAssertTrue(tunnels[1].colos.isEmpty)
    }

    /// Äldre utdata saknar `connections`. Posten ska bli en tunnel som är NERE,
    /// inte hoppas över — annars försvinner den ur listan helt.
    func testMissingConnectionsFieldYieldsADownTunnelNotADroppedRow() {
        let tunnels = CloudflareService.parseTunnels(#"[{"id": "x", "name": "gammal"}]"#)
        XCTAssertEqual(tunnels.count, 1)
        XCTAssertFalse(tunnels[0].isUp)
    }

    func testValidationRejectsInjection() throws {
        for good in ["homelab", "min-tunnel", "tunnel_1", "6ff42ae2-765d-4adf"] {
            XCTAssertNoThrow(try CloudflareService.validateTunnel(good), good)
        }
        for bad in ["", "min tunnel", "t; rm -rf /", "t$(id)", "t'"] {
            XCTAssertThrowsError(try CloudflareService.tunnelInfoCommand(bad), bad)
        }
    }

    func testServiceStateAndVersionAreReadFromTheCombinedOutput() {
        let active = CloudflareService.parseServiceStatus("active\ncloudflared version 2026.8.0")
        XCTAssertEqual(active.state, "active")
        XCTAssertEqual(active.version, "cloudflared version 2026.8.0")

        let inactive = CloudflareService.parseServiceStatus("inactive\n")
        XCTAssertEqual(inactive.state, "inactive")
        XCTAssertNil(inactive.version, "utan version ska fältet vara tomt, inte gissat")

        XCTAssertEqual(CloudflareService.parseServiceStatus("").state, "okänt")
    }
}

final class GitHubServiceTests: XCTestCase {
    /// `&&` och inte `;`: misslyckas katalogbytet ska gh inte köras alls.
    func testAFailedCDMustNotFallThroughToTheWrongRepo() throws {
        let cmd = try GitHubService.runsCommand(repoPath: "/srv/bastion", limit: 5)
        XCTAssertEqual(
            cmd,
            "cd '/srv/bastion' && gh run list --limit 5 --json name,status,conclusion,headBranch 2>&1"
        )
        XCTAssertTrue(cmd.contains("&&"))
        XCTAssertFalse(cmd.contains("; gh"), "semikolon hade kört gh ändå")
    }

    /// Katalognamn är godtyckliga; apostrof är det enda som bryter citatet.
    func testPathsWithSpacesWorkAndOnlyTheQuoteBreakingCharacterIsRejected() {
        XCTAssertNoThrow(try GitHubService.runsCommand(repoPath: "/home/a/mina repon/b", limit: 5))
        XCTAssertNoThrow(try GitHubService.runsCommand(repoPath: "/srv/repo$test;x", limit: 5))
        XCTAssertThrowsError(try GitHubService.runsCommand(repoPath: "/srv/it's/repo", limit: 5))
        XCTAssertThrowsError(try GitHubService.runsCommand(repoPath: "   ", limit: 5))
    }

    func testTheLimitIsClampedToSomethingGhAccepts() throws {
        XCTAssertTrue(try GitHubService.runsCommand(repoPath: "/r", limit: 0).contains("--limit 1"))
        XCTAssertTrue(try GitHubService.runsCommand(repoPath: "/r", limit: 9999).contains("--limit 50"))
    }

    /// En pågående körning har ingen slutsats. Att läsa tom slutsats som "inte
    /// misslyckad" vore rätt av fel skäl.
    func testARunningJobIsNotReportedAsPassing() {
        let out = """
        [{"name": "CI", "status": "in_progress", "conclusion": "", "headBranch": "main"},
         {"name": "CI", "status": "completed", "conclusion": "failure", "headBranch": "dev"},
         {"name": "CI", "status": "completed", "conclusion": "success", "headBranch": "main"}]
        """
        let runs = GitHubService.parseRuns(out)
        XCTAssertEqual(runs.count, 3)
        XCTAssertTrue(runs[0].isRunning)
        XCTAssertFalse(runs[0].failed, "pågående är inte misslyckad")
        XCTAssertTrue(runs[1].failed)
        XCTAssertFalse(runs[2].failed)
    }

    func testTimeoutsAndStartupFailuresCountAsFailures() {
        for conclusion in ["timed_out", "startup_failure", "failure"] {
            let out = #"[{"name": "CI", "status": "completed", "conclusion": "\#(conclusion)"}]"#
            XCTAssertTrue(GitHubService.parseRuns(out)[0].failed, conclusion)
        }
        for conclusion in ["success", "skipped", "cancelled", "neutral"] {
            let out = #"[{"name": "CI", "status": "completed", "conclusion": "\#(conclusion)"}]"#
            XCTAssertFalse(GitHubService.parseRuns(out)[0].failed, conclusion)
        }
    }

    /// BLOCKED (check inte grön ännu) och DIRTY (konflikt) kräver helt olika
    /// åtgärder — att slå ihop dem hade dolt vilken som gäller.
    func testBlockedAndDirtyAreBothNotReadyButStayDistinguishable() {
        let out = """
        [{"number": 1, "title": "Klar", "isDraft": false, "mergeStateStatus": "CLEAN"},
         {"number": 2, "title": "Väntar", "isDraft": false, "mergeStateStatus": "BLOCKED"},
         {"number": 3, "title": "Konflikt", "isDraft": false, "mergeStateStatus": "DIRTY"},
         {"number": 4, "title": "Utkast", "isDraft": true, "mergeStateStatus": "CLEAN"}]
        """
        let prs = GitHubService.parsePullRequests(out)
        XCTAssertEqual(prs.count, 4)
        XCTAssertTrue(prs[0].isReady)
        XCTAssertFalse(prs[1].isReady)
        XCTAssertEqual(prs[1].mergeable, "BLOCKED", "orsaken ska finnas kvar")
        XCTAssertEqual(prs[2].mergeable, "DIRTY")
        XCTAssertFalse(prs[3].isReady, "ett utkast är aldrig redo")
    }

    /// Framgångsraden är stabil mellan gh-versioner; felmeddelandena är det
    /// inte.
    func testAuthenticationIsDetectedByTheSuccessLine() {
        XCTAssertTrue(GitHubService.isAuthenticated("  ✓ Logged in to github.com account blixten85"))
        XCTAssertFalse(GitHubService.isAuthenticated("You are not logged into any GitHub hosts."))
        XCTAssertFalse(GitHubService.isAuthenticated(""))
    }

    func testNonJSONOutputYieldsNothing() {
        for bad in ["", "gh: command not found", #"{"inte": "array"}"#, "null"] {
            XCTAssertTrue(GitHubService.parseRuns(bad).isEmpty, bad)
            XCTAssertTrue(GitHubService.parsePullRequests(bad).isEmpty, bad)
        }
        XCTAssertTrue(GitHubService.parsePullRequests(#"[{"title": "utan nummer"}]"#).isEmpty)
    }
}
