import Foundation

// Cloudflare Tunnel via `cloudflared` över SSH. Port av
// LinuxApp/integrations/src/cloudflare.rs.
//
// `cloudflared` och INTE Cloudflares HTTP-API: VISION räknar upp Cloudflare
// bland plugins, i sällskap med Docker och Proxmox — saker som KÖR på en
// server man ansluter till. Att prata med api.cloudflare.com hade krävt en
// token att lagra, och svaret hade handlat om ett konto i stället för om
// värden.

public enum CloudflareError: Error, Sendable, Equatable {
    case invalidTunnelName(String)
}

public struct CloudflareConnection: Sendable, Equatable {
    /// Cloudflares kod för datacentret, t.ex. `ARN` (Stockholm).
    public let colo: String
    public let pendingReconnect: Bool

    public init(colo: String, pendingReconnect: Bool) {
        self.colo = colo
        self.pendingReconnect = pendingReconnect
    }
}

public struct CloudflareTunnel: Sendable, Equatable {
    public let id: String
    public let name: String
    public let connections: [CloudflareConnection]

    public init(id: String, name: String, connections: [CloudflareConnection]) {
        self.id = id
        self.name = name
        self.connections = connections
    }

    /// Förmedlar tunneln trafik just nu?
    ///
    /// Noll anslutningar betyder att den finns men är nere. En tunnel som bara
    /// väntar på återanslutning räknas inte som uppe heller — den tar ingen
    /// trafik under tiden.
    public var isUp: Bool { connections.contains { !$0.pendingReconnect } }

    /// Datacentren utan dubbletter. `cloudflared` öppnar normalt fyra
    /// anslutningar över två colos, och `ARN, ARN, HEL, HEL` vore brus.
    public var colos: [String] {
        var seen: [String] = []
        for c in connections where !c.colo.isEmpty && !seen.contains(c.colo) {
            seen.append(c.colo)
        }
        return seen
    }
}

public enum CloudflareService {
    /// Tunnelnamn får innehålla bokstäver, siffror, bindestreck och
    /// understreck. Ett id är en UUID, som matchar samma mönster.
    public static func validateTunnel(_ name: String) throws -> String {
        guard !name.isEmpty, name.count <= 64,
              name.allSatisfy({ ($0.isASCII && $0.isLetter) || ($0.isASCII && $0.isNumber) || $0 == "-" || $0 == "_" })
        else {
            throw CloudflareError.invalidTunnelName(name)
        }
        return name
    }

    public static func tunnelsCommand() -> String {
        "cloudflared tunnel list --output json 2>/dev/null"
    }

    public static func tunnelInfoCommand(_ name: String) throws -> String {
        "cloudflared tunnel info --output json \(try validateTunnel(name)) 2>&1"
    }

    /// Att tunneln finns i listan säger ingenting om huruvida daemonen kör —
    /// och det är just den skillnaden som förklarar en tunnel utan
    /// anslutningar.
    public static func serviceStatusCommand() -> String {
        "systemctl is-active cloudflared 2>&1; cloudflared --version 2>/dev/null"
    }

    /// Fälten läses defensivt: Cloudflare har lagt till fält mellan versioner,
    /// och en post utan `connections` (äldre utdata) ska bli en tunnel som är
    /// NERE, inte en post som hoppas över.
    public static func parseTunnels(_ output: String) -> [CloudflareTunnel] {
        guard let data = output.trimmingCharacters(in: .whitespacesAndNewlines).data(using: .utf8),
              let parsed = try? JSONSerialization.jsonObject(with: data),
              let list = parsed as? [[String: Any]]
        else { return [] }

        return list.compactMap { item in
            let name = item["name"] as? String ?? ""
            guard !name.isEmpty else { return nil }
            let raw = item["connections"] as? [[String: Any]] ?? []
            let connections = raw.map { c in
                CloudflareConnection(
                    colo: c["colo_name"] as? String ?? "",
                    pendingReconnect: c["is_pending_reconnect"] as? Bool ?? false
                )
            }
            return CloudflareTunnel(
                id: item["id"] as? String ?? "", name: name, connections: connections
            )
        }
    }

    /// `systemctl is-active` svarar med ETT ord (`active`, `inactive`,
    /// `failed`) — exitkoden går förlorad när två kommandon kedjas, så ordet
    /// är det vi går på.
    public static func parseServiceStatus(_ output: String) -> (state: String, version: String?) {
        let lines = output.split(whereSeparator: { $0 == "\n" || $0 == "\r" })
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
        let state = lines.first ?? "okänt"
        let version = lines.first { $0.contains("cloudflared") }
        return (state, version)
    }
}
