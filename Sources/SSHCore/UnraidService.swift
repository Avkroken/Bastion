import Foundation

// Unraid via `mdcmd` över SSH. Port av LinuxApp/integrations/src/unraid.rs.
//
// Tredje utdataformatet bland integrationerna: Docker/Kubernetes/Proxmox ger
// kolumner, TrueNAS ger JSON, och `mdcmd status` ger nyckel=värde där diskarna
// är INDEXERADE i nyckeln (diskName.0, diskSize.0, diskState.0).
//
// Diskarnas tillståndskoder visas RÅA. `diskState.N` är ett heltal vars
// betydelse inte är stabilt dokumenterad av Unraid och har ändrats mellan
// versioner. En påhittad översättningstabell hade gett en rad som SER
// auktoritativ ut men kan vara fel, vilket är sämre än en siffra man kan slå
// upp. `mdState` är däremot en sträng och tolkas.

public struct UnraidResync: Sendable, Equatable {
    public let position: UInt64
    public let total: UInt64

    public init(position: UInt64, total: UInt64) {
        self.position = position
        self.total = total
    }

    /// Andel klart, 0–1. `nil` när ingen resync pågår — noll som total betyder
    /// INGEN kontroll, inte "noll procent klart", som hade sett ut som en
    /// kontroll som står stilla.
    public var fraction: Double? {
        guard total > 0 else { return nil }
        return min(1.0, max(0.0, Double(position) / Double(total)))
    }
}

public struct UnraidArrayStatus: Sendable, Equatable {
    public let state: String
    public let diskCount: UInt32?
    public let disabledCount: UInt32?
    public let resync: UnraidResync?

    public init(state: String, diskCount: UInt32?, disabledCount: UInt32?, resync: UnraidResync?) {
        self.state = state
        self.diskCount = diskCount
        self.disabledCount = disabledCount
        self.resync = resync
    }

    public var isStarted: Bool { state == "STARTED" }

    /// En avstängd disk betyder att arrayen kör PÅ PARITET. Data finns kvar,
    /// men nästa diskfel är ett datafel — skillnaden mellan "allt är bra" och
    /// "åtgärda nu".
    public var hasDisabledDisks: Bool { (disabledCount ?? 0) > 0 }
}

public struct UnraidDisk: Sendable, Equatable {
    public let slot: UInt32
    public let name: String
    /// Storleken som `mdcmd` rapporterar den: i 1024-byteblock.
    public let sizeBlocks: UInt64
    /// Rå tillståndskod. Se filkommentaren om varför den inte tolkas.
    public let state: String

    public init(slot: UInt32, name: String, sizeBlocks: UInt64, state: String) {
        self.slot = slot
        self.name = name
        self.sizeBlocks = sizeBlocks
        self.state = state
    }

    public var sizeBytes: UInt64 { sizeBlocks.multipliedReportingOverflow(by: 1024).partialValue }
}

public enum UnraidService {
    public static func statusCommand() -> String { "mdcmd status 2>/dev/null" }

    /// Delade mappar är kataloger under `/mnt/user`. Unraid har ingen CLI som
    /// listar dem — webbgränssnittet läser `/boot/config/shares/*.cfg`.
    /// Katalogerna säger samma sak om vad som FINNS.
    public static func sharesCommand() -> String { "ls -1 /mnt/user 2>/dev/null" }

    /// Delar på FÖRSTA likhetstecknet: ett värde kan innehålla fler, och en
    /// naiv delning hade tappat allt efter det andra.
    static func pairs(_ output: String) -> [String: String] {
        var map: [String: String] = [:]
        for line in output.split(whereSeparator: { $0 == "\n" || $0 == "\r" }) {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard let index = trimmed.firstIndex(of: "=") else { continue }
            let key = String(trimmed[trimmed.startIndex..<index])
            guard !key.isEmpty else { continue }
            map[key] = String(trimmed[trimmed.index(after: index)...])
        }
        return map
    }

    /// Utan `mdState` är svaret inte från mdcmd. Att bygga en status av tomma
    /// fält hade gett en vy som ser fungerande ut mot en maskin som inte är en
    /// Unraid alls.
    public static func parseStatus(_ output: String) -> UnraidArrayStatus? {
        let map = pairs(output)
        guard let state = map["mdState"] else { return nil }
        let number: (String) -> UInt64? = { key in map[key].flatMap { UInt64($0) } }
        let total = number("mdResync") ?? 0
        return UnraidArrayStatus(
            state: state,
            diskCount: number("mdNumDisks").map { UInt32($0) },
            disabledCount: number("mdNumDisabled").map { UInt32($0) },
            resync: total > 0
                ? UnraidResync(position: number("mdResyncPos") ?? 0, total: total)
                : nil
        )
    }

    /// En disk räknas som närvarande först när den har ett NAMN. Unraid
    /// rapporterar tomma slottar med `diskName.N=`, och de ska inte bli rader.
    public static func parseDisks(_ output: String) -> [UnraidDisk] {
        let map = pairs(output)
        var disks: [UnraidDisk] = []
        for (key, value) in map {
            guard key.hasPrefix("diskName.") else { continue }
            let index = String(key.dropFirst("diskName.".count))
            guard let slot = UInt32(index) else { continue }
            let name = value.trimmingCharacters(in: .whitespaces)
            guard !name.isEmpty else { continue }
            disks.append(UnraidDisk(
                slot: slot,
                name: name,
                sizeBlocks: map["diskSize.\(slot)"].flatMap { UInt64($0) } ?? 0,
                state: map["diskState.\(slot)"] ?? ""
            ))
        }
        // Slotordning, inte hashordning — annars hoppar listan mellan
        // uppdateringar.
        return disks.sorted { $0.slot < $1.slot }
    }

    public static func parseShares(_ output: String) -> [String] {
        output.split(whereSeparator: { $0 == "\n" || $0 == "\r" })
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
    }

    // MARK: - Körning över SSH

    /// Array och diskar kommer ur SAMMA svar — två anrop hade varit två
    /// round-trips för samma data.
    public static func status(over session: SSHSession) async throws -> (UnraidArrayStatus?, [UnraidDisk]) {
        let output = try await session.run(statusCommand())
        return (parseStatus(output), parseDisks(output))
    }

    public static func shares(over session: SSHSession) async throws -> [String] {
        parseShares(try await session.run(sharesCommand()))
    }
}
