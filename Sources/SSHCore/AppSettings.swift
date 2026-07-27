import Foundation

/// Vilka valfria funktionsknappar en klient visar per värd (Docker, Snippets,
/// Kommandobibliotek, Filer, Tunnlar, SSH-nyckeldistribution). Alla standard
/// `true` så befintliga installationer inte tappar knappar vid uppgradering.
/// Persisteras separat från `HostStore` (`~/.bastion/settings.json`) eftersom
/// det är klientinställning, inte synkad värddata.
public struct FeatureToggles: Codable, Equatable, Sendable {
    public var showDocker: Bool
    public var showSnippets: Bool
    public var showCommandLibrary: Bool
    public var showSFTPBrowser: Bool
    public var showPortForward: Bool
    public var showKeyDeploy: Bool

    public init(
        showDocker: Bool = true,
        showSnippets: Bool = true,
        showCommandLibrary: Bool = true,
        showSFTPBrowser: Bool = true,
        showPortForward: Bool = true,
        showKeyDeploy: Bool = true
    ) {
        self.showDocker = showDocker
        self.showSnippets = showSnippets
        self.showCommandLibrary = showCommandLibrary
        self.showSFTPBrowser = showSFTPBrowser
        self.showPortForward = showPortForward
        self.showKeyDeploy = showKeyDeploy
    }
}

/// Trådsäker persistens för `FeatureToggles`. `path: nil` = endast i minne (test).
public final class AppSettingsStore {
    private let path: String?
    private let lock = NSLock()
    private var toggles: FeatureToggles

    public static var defaultPath: String {
        (("~/.bastion/settings.json") as NSString).expandingTildeInPath
    }

    public init(path: String? = AppSettingsStore.defaultPath) {
        self.path = path
        self.toggles = AppSettingsStore.load(path: path)
    }

    private static func load(path: String?) -> FeatureToggles {
        guard let path, let data = try? Data(contentsOf: URL(fileURLWithPath: path)),
              let decoded = try? JSONDecoder().decode(FeatureToggles.self, from: data)
        else { return FeatureToggles() }
        return decoded
    }

    public func current() -> FeatureToggles {
        lock.withLock { toggles }
    }

    public func update(_ newValue: FeatureToggles) {
        lock.withLock {
            toggles = newValue
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
        if let data = try? encoder.encode(toggles) {
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
