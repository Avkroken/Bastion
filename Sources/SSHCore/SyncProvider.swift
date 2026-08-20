import Foundation

/// Tak på hur stor en synkfil får vara innan den läses in i minnet.
///
/// Filen kommer per design från en mapp vi INTE kontrollerar — en delad
/// iCloud-/Dropbox-/Drive-katalog. Någon med skrivrättigheter där (eller ett
/// kapat molnkonto) kan lägga dit en fil på flera gigabyte, och
/// `Data(contentsOf:)` allokerar hela längden innan något ens tittat på
/// innehållet. På en telefon är det en omedelbar OOM-död.
///
/// Värdet är identiskt med LinuxApps `MAX_SYNC_FILE_BYTES`. 64 MiB är långt
/// över allt verkligt: ett tillstånd med tusen värdar och lika många snippets
/// ligger under en megabyte. Taket finns för att stänga en klass av angrepp,
/// inte för att begränsa användaren.
public enum SyncFileLimits {
    public static let maxBytes = 64 * 1024 * 1024

    /// Läser en fil men vägrar mer än ``maxBytes``.
    ///
    /// Läser via `read(upToCount: maxBytes + 1)` i stället för att först
    /// fråga om filstorleken och sedan läsa: en storlekskoll följd av en
    /// läsning är två separata operationer, och filen kan växa emellan. Här
    /// är gränsen en egenskap hos själva läsningen, så det finns inget glapp
    /// att utnyttja.
    public static func read(_ path: String) throws -> Data {
        let handle = try FileHandle(forReadingFrom: URL(fileURLWithPath: path))
        defer { try? handle.close() }
        let data = try handle.read(upToCount: maxBytes + 1) ?? Data()
        guard data.count <= maxBytes else {
            throw SyncFileError.tooLarge(maxBytes: maxBytes)
        }
        return data
    }
}

public enum SyncFileError: Error, Equatable, CustomStringConvertible {
    case tooLarge(maxBytes: Int)

    public var description: String {
        switch self {
        case .tooLarge(let maxBytes):
            return "synkfilen är större än \(maxBytes / 1024 / 1024) MiB och lästes inte in — "
                + "en fil den här storleken kommer inte från Bastion"
        }
    }
}


/// En synktransport: hämta fjärrtillstånd och skriv tillbaka det sammanslagna.
/// Medvetet minimal så olika ryggar kan implementeras — iCloud Drive, Dropbox,
/// Syncthing, en Git-mapp, WebDAV — utan att kärnan bryr sig om vilken.
public protocol SyncProvider: Sendable {
    func pull() throws -> SyncState?
    func push(_ state: SyncState) throws
}

/// Enklaste transporten: en JSON-fil i en mapp som något annat synkar mellan
/// enheter (iCloud Drive-behållare, Dropbox, Syncthing, en klonad Git-mapp …).
/// Ingen inloggning, ingen server — bara en fil.
public struct FolderSyncProvider: SyncProvider {
    private let path: String

    public init(path: String) {
        self.path = (path as NSString).expandingTildeInPath
    }

    public func pull() throws -> SyncState? {
        guard FileManager.default.fileExists(atPath: path) else { return nil }
        return try JSONDecoder().decode(SyncState.self, from: SyncFileLimits.read(path))
    }

    public func push(_ state: SyncState) throws {
        let dir = (path as NSString).deletingLastPathComponent
        try FileManager.default.createDirectory(
            atPath: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        try encoder.encode(state).write(to: URL(fileURLWithPath: path))
    }
}
