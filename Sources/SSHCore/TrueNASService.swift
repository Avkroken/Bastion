import Foundation

// TrueNAS via `midclt` över SSH. Port av LinuxApp/integrations/src/truenas.rs.
//
// `midclt` och inte `zpool`/`systemctl`: TrueNAS är ett appliance, och
// konfigurationen ägs av middleware-daemonen. Det som ändras utanför den
// skrivs över vid nästa omkonfiguration. `midclt call` är samma API som
// webbgränssnittet använder.

public enum TrueNASError: Error, Sendable, Equatable {
    case invalidServiceID(String)
}

public struct TrueNASPool: Sendable, Equatable {
    public let name: String
    public let status: String
    public let healthy: Bool

    public init(name: String, status: String, healthy: Bool) {
        self.name = name
        self.status = status
        self.healthy = healthy
    }

    /// `healthy` är INTE samma sak som `status == "ONLINE"`. En pool kan vara
    /// ONLINE och ändå ohälsosam — pågående resilver, läsfel som inte tagit ner
    /// en disk, checksummefel från en scrub. Att härleda hälsan ur
    /// statussträngen hade dolt precis de fallen.
    public var needsAttention: Bool { !healthy || status != "ONLINE" }
}

public struct TrueNASServiceState: Sendable, Equatable {
    /// Middlewares egen identifierare: `cifs`, `nfs`, `ssh`.
    public let id: String
    public let state: String
    public let enabled: Bool

    public init(id: String, state: String, enabled: Bool) {
        self.id = id
        self.state = state
        self.enabled = enabled
    }

    public var isRunning: Bool { state == "RUNNING" }

    /// Kör men startar inte vid uppstart — överlever alltså inte en omstart.
    /// Nästan alltid oavsiktligt.
    public var isRunningButNotEnabled: Bool { isRunning && !enabled }
}

public struct TrueNASAlert: Sendable, Equatable {
    public let level: String
    public let formatted: String
    public let dismissed: Bool

    public init(level: String, formatted: String, dismissed: Bool) {
        self.level = level
        self.formatted = formatted
        self.dismissed = dismissed
    }

    /// CRITICAL och ERROR betyder att något är trasigt nu. WARNING och nedåt
    /// är information.
    public var isCritical: Bool {
        ["CRITICAL", "ERROR", "ALERT", "EMERGENCY"].contains(level)
    }
}

public enum TrueNASService {
    /// Tjänste-id:n är korta gemena ord (`cifs`, `nfs`, `smartd`). Snäv med
    /// flit: den ska släppa igenom exakt de namnen, vilket gör den till ett
    /// fullgott injektionsskydd på köpet.
    public static func validateService(_ id: String) throws -> String {
        guard !id.isEmpty, id.count <= 32,
              id.allSatisfy({ ($0.isASCII && $0.isLowercase) || ($0.isASCII && $0.isNumber) || $0 == "_" })
        else {
            throw TrueNASError.invalidServiceID(id)
        }
        return id
    }

    public static func poolsCommand() -> String { "midclt call pool.query 2>/dev/null" }
    public static func servicesCommand() -> String { "midclt call service.query 2>/dev/null" }
    public static func alertsCommand() -> String { "midclt call alert.list 2>/dev/null" }

    /// Argumentet till `midclt` är JSON, alltså en CITERAD sträng inuti
    /// skalcitationen. Att valideringen uteslutit apostrof är det som gör den
    /// konstruktionen säker.
    static func serviceCommand(_ verb: String, _ id: String) throws -> String {
        "midclt call service.\(verb) '\"\(try validateService(id))\"' 2>&1"
    }

    public static func startServiceCommand(_ id: String) throws -> String {
        try serviceCommand("start", id)
    }

    public static func stopServiceCommand(_ id: String) throws -> String {
        try serviceCommand("stop", id)
    }

    public static func restartServiceCommand(_ id: String) throws -> String {
        try serviceCommand("restart", id)
    }

    /// `midclt` svarar med en JSON-array vid framgång. Vid fel skriver den ett
    /// traceback på stderr — som gått till /dev/null — och inget på stdout, så
    /// tomt in ska ge tomt ut.
    static func array(_ output: String) -> [[String: Any]] {
        guard let data = output.trimmingCharacters(in: .whitespacesAndNewlines).data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data),
              let list = parsed as? [[String: Any]]
        else { return [] }
        return list
    }

    static func text(_ item: [String: Any], _ key: String) -> String {
        item[key] as? String ?? ""
    }

    public static func parsePools(_ output: String) -> [TrueNASPool] {
        array(output).compactMap { item in
            let name = text(item, "name")
            guard !name.isEmpty else { return nil }
            // Saknat `healthy` tolkas som OHÄLSOSAMT: en äldre middleware utan
            // fältet ska ge en varning att titta närmare, inte ett tyst
            // godkännande.
            return TrueNASPool(
                name: name,
                status: text(item, "status"),
                healthy: item["healthy"] as? Bool ?? false
            )
        }
    }

    public static func parseServices(_ output: String) -> [TrueNASServiceState] {
        array(output).compactMap { item in
            let id = text(item, "service")
            guard !id.isEmpty else { return nil }
            return TrueNASServiceState(
                id: id, state: text(item, "state"), enabled: item["enable"] as? Bool ?? false
            )
        }
    }

    public static func parseAlerts(_ output: String) -> [TrueNASAlert] {
        array(output).compactMap { item in
            let formatted = text(item, "formatted")
            guard !formatted.isEmpty else { return nil }
            return TrueNASAlert(
                level: text(item, "level"),
                formatted: formatted,
                dismissed: item["dismissed"] as? Bool ?? false
            )
        }
    }
}
