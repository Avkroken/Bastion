import Foundation

/// Persistent snippet-databas (JSON på disk, samma mönster som `HostStore`
/// men utan sync-integration — v1 håller det enkelt, se ROADMAP.md).
/// Standardplats: `~/.bastion/snippets.json`. `path: nil` = endast i minne (test).
public final class SnippetStore {
    private let path: String?
    private let lock = NSLock()
    private var byID: [UUID: Snippet]

    public static var defaultPath: String {
        (("~/.bastion/snippets.json") as NSString).expandingTildeInPath
    }

    public init(path: String? = SnippetStore.defaultPath) {
        self.path = path
        self.byID = Dictionary(
            SnippetStore.load(path: path).map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
    }

    private static func load(path: String?) -> [Snippet] {
        guard let path, let data = try? Data(contentsOf: URL(fileURLWithPath: path)) else { return [] }
        return (try? JSONDecoder().decode([Snippet].self, from: data)) ?? []
    }

    /// Alla snippets, sorterade på namn (skiftlägesokänsligt).
    public func all() -> [Snippet] {
        lock.withLock { Array(byID.values) }
            .sorted { $0.name.lowercased() < $1.name.lowercased() }
    }

    public func get(_ id: UUID) -> Snippet? {
        lock.withLock { byID[id] }
    }

    /// Lägg till eller uppdatera. Stämplar `modifiedAt = nu`.
    public func upsert(_ snippet: Snippet) {
        lock.withLock {
            var s = snippet
            s.modifiedAt = Date()
            byID[s.id] = s
            persist()
        }
    }

    public func delete(_ id: UUID) {
        lock.withLock {
            byID[id] = nil
            persist()
        }
    }

    /// Raderar OCH skriver en gravsten i sync-tillståndet, så raderingen
    /// överlever en synk. Använd den här i allt som kan synkas — utan
    /// gravsten kommer motparten glatt tillbaka med sin kopia, och snippeten
    /// återuppstår varje gång användaren raderar den.
    ///
    /// Gravstenarna bor i `HostStore` och inte här: de betyder bara något för
    /// synken, och en delad karta för båda posttyperna slipper både ett andra
    /// fält på tråden och en formatändring av snippet-filen som varje
    /// plattform hade behövt följa.
    public func delete(_ id: UUID, recordingTombstoneIn hosts: HostStore) {
        delete(id)
        hosts.recordTombstone(id)
    }

    /// Ersätter hela innehållet — används av synken när det sammanslagna
    /// resultatet ska skrivas tillbaka. Rör INTE `modifiedAt`, till skillnad
    /// från `upsert`: tidsstämplarna kommer från hopslagningen och är precis
    /// det som avgjorde vem som vann.
    public func replaceAll(_ snippets: [Snippet]) {
        lock.withLock {
            byID = Dictionary(snippets.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            persist()
        }
    }

    // Anropas med låset hållet.
    private func persist() {
        guard let path else { return }
        let dir = (path as NSString).deletingLastPathComponent
        try? FileManager.default.createDirectory(
            atPath: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        if let data = try? encoder.encode(Array(byID.values).sorted { $0.name < $1.name }) {
            try? data.write(to: URL(fileURLWithPath: path), options: .atomic)
        }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () -> T) -> T {
        lock(); defer { unlock() }
        return body()
    }
}
