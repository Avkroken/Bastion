import Foundation

/// Hur en sparad värd autentiseras. Hemligheter lagras INTE här — lösenord hör
/// hemma i Keychain (iOS/macOS), och `keyFile` pekar bara på en nyckel på disk.
public enum HostAuth: Codable, Sendable, Equatable {
    case askPassword            // fråga vid varje anslutning (ev. via Keychain)
    case keyFile(String)        // sökväg till en (okrypterad) privatnyckel
    case agentDefault           // ~/.ssh/id_ed25519 / ssh-config
    case keychainKey(String)    // nyckelmaterial importerat i appen, id i Keychain
    /// OpenSSH-certifikatautentisering: privatnyckelns sökväg + det
    /// signerade certifikatets sökväg (typiskt `<nyckel>-cert.pub`,
    /// skrivet av `ssh-keygen -s`).
    case certificateFile(keyPath: String, certPath: String)
    /// Lösenord hämtas vid varje anslutning ur en lokal Bitwarden CLI
    /// (`bw get password <id>`) — se `BitwardenClient`. `String`et är
    /// Bitwardens item-id eller unika namn, inte lösenordet självt.
    case bitwardenItem(String)
}

/// En sparad värd i host-databasen. Ren metadata (inga hemligheter) så den kan
/// synkas och säkerhetskopieras fritt. Taggar i stället för enbart mappar.
public struct Host: Codable, Identifiable, Sendable, Equatable {
    public var id: UUID
    public var alias: String
    public var hostName: String
    public var user: String
    public var port: Int
    public var tags: [String]
    public var auth: HostAuth
    public var isFavorite: Bool
    public var colorTag: String?
    /// Vilken sorts fjärrsystem den här värden är — styr bara hur
    /// `deployPublicKey` bygger sitt kommando (POSIX-skal vs. Windows
    /// PowerShell, admin- vs. standardkonto). Påverkar ingenting annat;
    /// `.posix` (default) fungerar precis som innan fältet fanns.
    public var platform: RemotePlatform
    /// Körs automatiskt i skalet direkt efter att en INTERAKTIV terminal
    /// öppnats för den här värden (inte vid `execute()`-baserade engångs-
    /// kommandon som Docker-shell/Snippets — de skickar redan sitt eget
    /// `initialCommand` och ska inte dubbelköras). Motsvarar Termius
    /// "Startup Snippet". `nil`/tomt = ingenting körs (samma beteende som
    /// innan fältet fanns).
    public var startupCommand: String?
    /// Id på en annan `Host` i samma store att ansluta GENOM (ssh -J/ProxyJump)
    /// innan denna värd nås — se `SSHSession.connect(via:)`. `nil` (default)
    /// = direkt anslutning, precis som innan fältet fanns. Får inte peka på
    /// sig själv eller bilda en cykel; UI/anropskod ansvarar för att
    /// validera det (modellen tillåter det tekniskt, som `Host.imported`
    /// redan gör med andra ogiltiga tillstånd).
    public var jumpHostID: UUID?
    /// MAC-adress för Wake-on-LAN (`WakeOnLan.send`), t.ex. `AA:BB:CC:DD:EE:FF`.
    /// `nil` (default) = ingen WoL-knapp visas för värden, precis som innan
    /// fältet fanns. Sparas ovaliderad — `WakeOnLan.parseMAC` validerar vid
    /// användningstillfället, inte vid lagring (samma mönster som `hostName`
    /// inte validerar DNS-syntax vid sparning).
    public var macAddress: String?
    /// Vidarebefordra den lokala ssh-agenten till värden (OpenSSH:s
    /// `ForwardAgent`).
    ///
    /// FALSKT som förval, och det är ett säkerhetsval snarare än ett
    /// bekvämlighetsval: med agenten vidarebefordrad kan vem som helst med
    /// root på fjärrvärden använda DINA nycklar så länge sessionen lever —
    /// utan att kunna läsa dem, men utan att du märker något heller.
    /// OpenSSH har samma förval av samma skäl.
    ///
    /// Fältet fanns i LinuxApps `Host` men saknades här, och `Codable`
    /// släpper okända nycklar tyst. Följden var att en värd med
    /// agentvidarebefordran påslagen FÖRLORADE inställningen så fort
    /// tillståndet passerade en Apple-enhet vid synk — avkodningen kastade
    /// nyckeln, kodningen skrev inte tillbaka den. Ingen felutskrift, ingen
    /// synlig ändring förrän nästa anslutning betedde sig annorlunda.
    public var forwardAgent: Bool
    /// Adress (`värd:port`) till en SOCKS5-proxy som anslutningen ska gå
    /// GENOM. `nil` = anslut direkt, precis som innan fältet fanns.
    ///
    /// Två verkliga användningar: en företagsproxy, och `tailscaled
    /// --tun=userspace-networking --socks5-server=…`, som exponerar hela
    /// tailnet:et utan att kräva ett TUN-gränssnitt. Målets namn slås upp i
    /// PROXYN, inte lokalt.
    ///
    /// Skilt från ``jumpHostID``: en jump-host är en SSH-server vi
    /// autentiserar mot och tunnlar genom, en SOCKS-proxy är ren
    /// TCP-transport utan egen inloggning.
    ///
    /// LinuxApp ANVÄNDER fältet redan (`ssh::connect_direct`). Här bärs det
    /// tills vidare bara genom modellen och synken — utan det skulle
    /// inställningen raderas så fort tillståndet passerade en Apple-enhet,
    /// exakt den bugg `forwardAgent` och `jumpHostID` redan orsakat.
    public var socksProxy: String?
    /// När värden senast ändrades. Styr sync-mergen (nyaste ändringen vinner).
    public var modifiedAt: Date

    public init(
        id: UUID = UUID(),
        alias: String,
        hostName: String,
        user: String,
        port: Int = 22,
        tags: [String] = [],
        auth: HostAuth = .agentDefault,
        isFavorite: Bool = false,
        colorTag: String? = nil,
        platform: RemotePlatform = .posix,
        startupCommand: String? = nil,
        jumpHostID: UUID? = nil,
        macAddress: String? = nil,
        forwardAgent: Bool = false,
        socksProxy: String? = nil,
        modifiedAt: Date = Date()
    ) {
        self.id = id
        self.alias = alias
        self.hostName = hostName
        self.user = user
        self.port = port
        self.tags = tags
        self.auth = auth
        self.isFavorite = isFavorite
        self.colorTag = colorTag
        self.platform = platform
        self.startupCommand = startupCommand
        self.jumpHostID = jumpHostID
        self.macAddress = macAddress
        self.forwardAgent = forwardAgent
        self.socksProxy = socksProxy
        self.modifiedAt = modifiedAt
    }

    private enum CodingKeys: String, CodingKey {
        case id, alias, hostName, user, port, tags, auth, isFavorite, colorTag, platform, startupCommand, jumpHostID, macAddress, forwardAgent, socksProxy, modifiedAt
        /// LinuxApp skrev fältet som `jumpHostId` (serdes `camelCase` av
        /// `jump_host_id`) medan den här sidan alltid skrivit `jumpHostID`
        /// (Apples konvention versaliserar initialförkortningar). Nycklarna
        /// matchade alltså inte, och både `Codable` och serde släpper okända
        /// nycklar TYST — följden var att en ProxyJump-koppling försvann i
        /// båda riktningarna så fort tillståndet synkades mellan en Linux-
        /// och en Apple-enhet. Värst tänkbara fält att tappa: målet är ofta
        /// bara nåbart genom hoppet.
        ///
        /// Båda sidor skriver nu `jumpHostID`. Den här nyckeln finns kvar för
        /// att LÄSA redan sparade filer, aldrig för att skriva.
        case legacyJumpHostID = "jumpHostId"
    }

    /// Egen init(from:) — isFavorite/colorTag/platform/startupCommand/
    /// jumpHostID/macAddress tillkom efter att fältet fanns i sparade
    /// host.json-filer. `decodeIfPresent` gör dem valfria vid avkodning
    /// (default false/nil/.posix/nil/nil/nil) istället för att synteterad
    /// Decodable kastar på saknad nyckel.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        alias = try c.decode(String.self, forKey: .alias)
        hostName = try c.decode(String.self, forKey: .hostName)
        user = try c.decode(String.self, forKey: .user)
        port = try c.decode(Int.self, forKey: .port)
        tags = try c.decode([String].self, forKey: .tags)
        auth = try c.decode(HostAuth.self, forKey: .auth)
        isFavorite = try c.decodeIfPresent(Bool.self, forKey: .isFavorite) ?? false
        colorTag = try c.decodeIfPresent(String.self, forKey: .colorTag)
        platform = try c.decodeIfPresent(RemotePlatform.self, forKey: .platform) ?? .posix
        startupCommand = try c.decodeIfPresent(String.self, forKey: .startupCommand)
        jumpHostID = try c.decodeIfPresent(UUID.self, forKey: .jumpHostID)
            ?? c.decodeIfPresent(UUID.self, forKey: .legacyJumpHostID)
        macAddress = try c.decodeIfPresent(String.self, forKey: .macAddress)
        forwardAgent = try c.decodeIfPresent(Bool.self, forKey: .forwardAgent) ?? false
        socksProxy = try c.decodeIfPresent(String.self, forKey: .socksProxy)
        modifiedAt = try c.decode(Date.self, forKey: .modifiedAt)
    }

    /// Egen `encode(to:)` av ETT skäl: den syntetiserade varianten skriver ut
    /// varje `CodingKey`, och `legacyJumpHostID` finns bara för att LÄSA gamla
    /// filer. Utan den här skulle varje sparning skriva båda stavningarna, och
    /// nästa läsare få två källor till samma sanning.
    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(alias, forKey: .alias)
        try c.encode(hostName, forKey: .hostName)
        try c.encode(user, forKey: .user)
        try c.encode(port, forKey: .port)
        try c.encode(tags, forKey: .tags)
        try c.encode(auth, forKey: .auth)
        try c.encode(isFavorite, forKey: .isFavorite)
        try c.encodeIfPresent(colorTag, forKey: .colorTag)
        try c.encode(platform, forKey: .platform)
        try c.encodeIfPresent(startupCommand, forKey: .startupCommand)
        try c.encodeIfPresent(jumpHostID, forKey: .jumpHostID)
        try c.encodeIfPresent(macAddress, forKey: .macAddress)
        try c.encode(forwardAgent, forKey: .forwardAgent)
        try c.encodeIfPresent(socksProxy, forKey: .socksProxy)
        try c.encode(modifiedAt, forKey: .modifiedAt)
    }

    /// Anslutningsmål för `SSHSession`.
    public var target: SSHTarget {
        SSHTarget(host: hostName, port: port, username: user)
    }

    /// Bygger värdar ur en `~/.ssh/config`. Varje konkret `Host`-alias blir en
    /// post med upplösta HostName/User/Port/IdentityFile. Alias utan användare
    /// hoppas över (kan inte anslutas ändå).
    public static func imported(from config: SSHConfig) -> [Host] {
        config.hostAliases.compactMap { alias in
            let r = config.resolve(alias)
            guard let user = r.user, !user.isEmpty else { return nil }
            let auth: HostAuth = r.identityFile.map { .keyFile($0) } ?? .agentDefault
            // Fälten fanns redan i modellen — importen fyllde dem bara aldrig
            // i, så en användare som konfigurerat dem i ssh-config fick dem
            // tyst bortkastade och en värd som betedde sig annorlunda här än
            // under `ssh`.
            let startup = r.remoteCommand.flatMap { $0.isEmpty ? nil : $0 }
            return Host(
                alias: alias, hostName: r.hostName, user: user, port: r.port, auth: auth,
                startupCommand: startup, forwardAgent: r.forwardAgent)
        }
    }
}
