import Foundation

// Proxmox VE via `qm`, `pct` och `pvesm` över SSH. Port av
// LinuxApp/integrations/src/proxmox.rs.
//
// `qm`/`pct`/`pvesm` och inte `pvesh`: det sistnämnda ger JSON och vore
// enhetligt, men kräver att man vet nodnamnet i sökvägen — och det man vill
// visa är vad som finns på noden man loggat in på.

public enum ProxmoxError: Error, Sendable, Equatable {
    case invalidVMID(String)
}

/// KVM-maskin eller LXC-container. Skillnaden är vilket verktyg som styr dem.
public enum ProxmoxGuestKind: Sendable, Equatable {
    case vm
    case container

    var tool: String {
        switch self {
        case .vm: return "qm"
        case .container: return "pct"
        }
    }
}

public struct ProxmoxGuest: Sendable, Equatable {
    public let vmid: String
    public let name: String
    public let status: String
    public let kind: ProxmoxGuestKind

    public init(vmid: String, name: String, status: String, kind: ProxmoxGuestKind) {
        self.vmid = vmid
        self.name = name
        self.status = status
        self.kind = kind
    }

    public var isRunning: Bool { status == "running" }
}

public struct ProxmoxStorage: Sendable, Equatable {
    public let name: String
    public let kind: String
    public let status: String
    /// Använt i procent som text (`73.31%`). Lämnas oparsat: siffran är till
    /// för att läsas, och ett tolkningsfel vore värre än Proxmox egen
    /// formatering.
    public let usedPercent: String

    public init(name: String, kind: String, status: String, usedPercent: String) {
        self.name = name
        self.kind = kind
        self.status = status
        self.usedPercent = usedPercent
    }

    public var isActive: Bool { status == "active" }
}

public enum ProxmoxService {
    /// Proxmox adresserar allt med ett VMID — ett heltal. 1–99 är reserverade
    /// internt; användarskapade gäster börjar på 100.
    ///
    /// En regel som bara accepterar siffror är samtidigt det starkaste
    /// injektionsskyddet bland integrationerna.
    public static func validateVMID(_ vmid: String) throws -> String {
        guard vmid.count <= 9,
              !vmid.isEmpty,
              vmid.allSatisfy({ $0.isASCII && $0.isNumber }),
              let n = Int(vmid), n >= 100
        else {
            throw ProxmoxError.invalidVMID(vmid)
        }
        return vmid
    }

    public static func vmsCommand() -> String { "qm list 2>/dev/null" }
    public static func containersCommand() -> String { "pct list 2>/dev/null" }
    public static func storageCommand() -> String { "pvesm status 2>/dev/null" }

    public static func startCommand(_ kind: ProxmoxGuestKind, _ vmid: String) throws -> String {
        "\(kind.tool) start \(try validateVMID(vmid)) 2>&1"
    }

    /// Ren avstängning via gästens eget OS. `shutdown` och inte `stop`: det
    /// senare motsvarar att dra ur strömmen och riskerar filsystemsskador.
    public static func shutdownCommand(_ kind: ProxmoxGuestKind, _ vmid: String) throws -> String {
        "\(kind.tool) shutdown \(try validateVMID(vmid)) 2>&1"
    }

    /// Hård avstängning. Motsvarar att dra ur strömmen.
    public static func stopCommand(_ kind: ProxmoxGuestKind, _ vmid: String) throws -> String {
        "\(kind.tool) stop \(try validateVMID(vmid)) 2>&1"
    }

    public static func configCommand(_ kind: ProxmoxGuestKind, _ vmid: String) throws -> String {
        "\(kind.tool) config \(try validateVMID(vmid)) 2>&1"
    }

    static func fields(_ line: Substring, _ expected: Int) -> [String]? {
        let parts = line.split(whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
        return parts.count >= expected ? parts : nil
    }

    /// Alla tre verktygen skriver en rubrikrad, och `--no-headers` finns inte.
    static func isHeader(_ first: String) -> Bool {
        first == "VMID" || first == "Name"
    }

    /// `qm list` ger VMID NAME STATUS MEM BOOTDISK PID — fasta kolumner.
    public static func parseVMs(_ output: String) -> [ProxmoxGuest] {
        output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            guard let f = fields(line, 3), !isHeader(f[0]),
                  let vmid = try? validateVMID(f[0]) else { return nil }
            return ProxmoxGuest(vmid: vmid, name: f[1], status: f[2], kind: .vm)
        }
    }

    /// `pct list` ger VMID Status Lock Name — och `Lock` är TOMT för en gäst
    /// som inte är låst. Fältantalet varierar därför mellan tre och fyra, och
    /// namnet hämtas BAKIFRÅN. Ett gästnamn är ett värdnamn och kan inte
    /// innehålla mellanslag, så sista fältet är alltid hela namnet.
    public static func parseContainers(_ output: String) -> [ProxmoxGuest] {
        output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            guard let f = fields(line, 3), !isHeader(f[0]),
                  let vmid = try? validateVMID(f[0]) else { return nil }
            return ProxmoxGuest(vmid: vmid, name: f[f.count - 1], status: f[1], kind: .container)
        }
    }

    /// `pvesm status` ger Name Type Status Total Used Available %.
    public static func parseStorage(_ output: String) -> [ProxmoxStorage] {
        output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }).compactMap { line in
            guard let f = fields(line, 7), !isHeader(f[0]) else { return nil }
            return ProxmoxStorage(
                name: f[0], kind: f[1], status: f[2], usedPercent: f[f.count - 1]
            )
        }
    }
}
