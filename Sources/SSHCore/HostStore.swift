import Foundation

/// Persistent host-databas (JSON på disk). Trådsäker. Taggbaserad filtrering.
/// Standardplats: `~/.bastion/hosts.json`. `path: nil` = endast i minne (test).
///
/// Persisterar ett `SyncState` (värdar + gravstenar) så databasen kan slås ihop
/// mellan enheter via `SyncEngine`. JSON medvetet (inte SQLite): diff-bart,
/// synkbart, inga systemberoenden — funkar identiskt på iOS/macOS/Linux/Windows.
public final class HostStore {
    private let path: String?
    private let lock = NSLock()
    private var byID: [UUID: Host]
    private var tombstones: [UUID: Date]

    public static var defaultPath: String {
        (("~/.bastion/hosts.json") as NSString).expandingTildeInPath
    }

    public init(path: String? = HostStore.defaultPath) {
        self.path = path
        let state = HostStore.load(path: path)
        self.byID = Dictionary(state.hosts.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
        self.tombstones = state.tombstones
    }

    private static func load(path: String?) -> SyncState {
        guard let path, let data = try? Data(contentsOf: URL(fileURLWithPath: path)) else {
            return SyncState()
        }
        // Nytt format (SyncState) först; annars äldre rena [Host]-listor.
        if let state = try? JSONDecoder().decode(SyncState.self, from: data) {
            return state
        }
        if let hosts = try? JSONDecoder().decode([Host].self, from: data) {
            return SyncState(hosts: hosts)
        }
        return SyncState()
    }

    /// Alla värdar, sorterade på alias (skiftlägesokänsligt).
    public func all() -> [Host] {
        lock.withLock { Array(byID.values) }
            .sorted { $0.alias.lowercased() < $1.alias.lowercased() }
    }

    public func get(_ id: UUID) -> Host? {
        lock.withLock { byID[id] }
    }

    /// Lägg till eller uppdatera. Stämplar `modifiedAt = nu` så en lokal ändring
    /// vinner i mergen, och rensar en eventuell gravsten (återupplivning).
    public func upsert(_ host: Host) {
        lock.withLock {
            var h = host
            h.modifiedAt = Date()
            byID[h.id] = h
            tombstones[h.id] = nil
            persist()
        }
    }

    public func delete(_ id: UUID) {
        lock.withLock {
            byID[id] = nil
            tombstones[id] = Date()      // gravsten så raderingen syncar
            persist()
        }
    }

    /// Värdar som bär en viss tagg.
    public func hosts(withTag tag: String) -> [Host] {
        all().filter { $0.tags.contains(tag) }
    }

    /// Alla taggar som används, unika och sorterade.
    public func allTags() -> [String] {
        let tags = lock.withLock { byID.values.flatMap { $0.tags } }
        return Array(Set(tags)).sorted { $0.lowercased() < $1.lowercased() }
    }

    /// Nuvarande tillstånd att skriva till en synktransport.
    public func exportState() -> SyncState {
        lock.withLock { SyncState(hosts: Array(byID.values), tombstones: tombstones) }
    }

    /// Slår ihop ett fjärrtillstånd (från en annan enhet) med det lokala och
    /// persisterar resultatet. Returnerar det sammanslagna tillståndet.
    @discardableResult
    public func merge(_ remote: SyncState, snippets: SnippetStore? = nil) -> SyncState {
        // Snippet-databasen läses UTANFÖR låset: den har ett eget lås, och att
        // ta två lås i den här ordningen någon annanstans skulle vara en
        // dödläges-risk som inte behöver finnas.
        let localSnippets = snippets?.all() ?? []
        let merged: SyncState = lock.withLock {
            let merged = SyncEngine.merge(
                SyncState(
                    hosts: Array(byID.values), snippets: localSnippets, tombstones: tombstones),
                remote)
            byID = Dictionary(merged.hosts.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            tombstones = merged.tombstones
            persist()
            return merged
        }
        // Snippets bor i sin egen fil — `hosts.json` ska inte börja bära dem
        // bara för att synken hanterar dem. Två sanningskällor för samma data
        // är hur de hinner glida isär.
        snippets?.replaceAll(merged.snippets)
        return merged
    }

    /// Importerar värdar från en `~/.ssh/config`-text. Alias som redan finns
    /// (skiftlägesokänsligt) hoppas över så re-import inte dubblerar. Returnerar
    /// de faktiskt tillagda värdarna.
    @discardableResult
    public func importSSHConfig(_ text: String) -> [Host] {
        let config = SSHConfig(text: text)
        let existing = Set(all().map { $0.alias.lowercased() })
        let fresh = Host.imported(from: config)
            .filter { !existing.contains($0.alias.lowercased()) }
        for host in fresh { upsert(host) }

        // Andra pass: koppla ihop ProxyJump. Måste ske EFTER att alla värdar
        // sparats — jump-hosten kan stå efter den som pekar på den i configen,
        // och en koppling kräver mottagarens id.
        //
        // Bara nyimporterade värdar får sin jump satt. Att röra en värd som
        // redan fanns vore att tyst skriva om något användaren kan ha ställt
        // in själv, och hela importen bygger på att befintliga alias lämnas
        // ifred.
        //
        // ProxyJump är den av de importerade inställningarna som märks mest
        // när den tappas: målet är ofta bara nåbart genom hoppet, så utan
        // kopplingen fungerar anslutningen inte alls.
        var byAlias: [String: UUID] = [:]
        for host in all() { byAlias[host.alias.lowercased()] = host.id }
        var linked: [Host] = []
        for host in fresh {
            guard let raw = config.resolve(host.alias).proxyJump,
                  let jumpAlias = SSHConfig.proxyJumpAlias(raw),
                  let jumpID = byAlias[jumpAlias.lowercased()],
                  jumpID != host.id  // en värd som pekar på sig själv är bara ett fel
            else {
                linked.append(host)
                continue
            }
            var updated = host
            updated.jumpHostID = jumpID
            upsert(updated)
            linked.append(updated)
        }
        return linked
    }

    /// Full synkrunda mot en transport: hämta fjärrtillstånd, slå ihop lokalt,
    /// skriv tillbaka det sammanslagna. Kör den när appen öppnas/backgrundas.
    ///
    /// Tar BÅDA databaserna. Två synkvägar skulle betyda att någon förr eller
    /// senare väljer den som tyst hoppar över snippets. De delar dessutom
    /// gravstenskarta, så två separata rundturer vore inte bara långsammare
    /// utan fel: den andra pushen skulle skriva över den förstas gravstenar.
    @discardableResult
    public func sync(with provider: SyncProvider, snippets: SnippetStore) throws -> SyncState {
        let remote = (try provider.pull()) ?? SyncState()
        let merged = merge(remote, snippets: snippets)
        try provider.push(merged)
        return merged
    }

    /// Skriver en gravsten utan att röra värdlistan — för poster som lever i
    /// en annan databas men delar sync-tillstånd (se `SnippetStore`).
    public func recordTombstone(_ id: UUID) {
        lock.withLock {
            tombstones[id] = Date()
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
        let state = SyncState(hosts: Array(byID.values).sorted { $0.alias < $1.alias },
                              tombstones: tombstones)
        if let data = try? encoder.encode(state) {
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
