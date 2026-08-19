import Foundation

/// Uppslaget resultat för ett värdalias ur `~/.ssh/config`.
public struct ResolvedHost: Sendable, Equatable {
    public var hostName: String
    public var user: String?
    public var port: Int
    public var identityFile: String?
    public var proxyJump: String?
    /// OpenSSH:s `ForwardAgent`. Bara ett uttryckligt ja räknas — allt annat,
    /// inklusive nyckelns frånvaro, är nej. Att gissa fel åt det hållet vore
    /// att slå på agentvidarebefordran åt någon som inte bett om det, och då
    /// kan vem som helst med root på fjärrvärden använda deras nycklar så
    /// länge sessionen lever.
    public var forwardAgent: Bool = false
    /// OpenSSH:s `RemoteCommand` — kommandot som körs direkt efter anslutning.
    /// Motsvarar ``Host/startupCommand``.
    public var remoteCommand: String?
}

/// Minimal läsare av OpenSSH:s klientkonfiguration (`~/.ssh/config`). Stöder
/// `Host`-block med jokertecken (`*`, `?`) och negation (`!`), `Include`, samt
/// de vanligaste nycklarna. Semantik enligt OpenSSH: **första värdet vinner**
/// per nyckel. `Match` stöds för de kriterier som går att avgöra utan en
/// pågående anslutning (`all`, `host`); allt annat lämnar blocket inaktivt —
/// se ``matchIsActive(_:_:)``.
public struct SSHConfig: Sendable {
    private enum Entry: Sendable {
        case host([String])
        /// Ett `Match`-block, med kriterieraden bevarad rå. Utvärderas först
        /// i `resolve`, eftersom `host`-kriteriet beror på vilket alias som
        /// slås upp — till skillnad från `host`, vars mönster står i posten.
        case match(String)
        case setting(String, String)
    }
    private let entries: [Entry]

    public static var defaultPath: String {
        (("~/.ssh/config") as NSString).expandingTildeInPath
    }

    /// Största antal `Include`-nivåer som följs. Samma gräns som OpenSSH:s
    /// egen (`readconf.c`, `MAX_READCONF_DEPTH` = 16), och av samma skäl: en
    /// config som direkt eller indirekt inkluderar sig själv ska ge en
    /// trunkerad läsning, inte en oändlig loop.
    private static let maxIncludeDepth = 16

    private init(entries: [Entry]) {
        self.entries = entries
    }

    /// Läser en config-TEXT utan att röra filsystemet. `Include`-rader hoppas
    /// över — det finns ingen katalog att lösa dem mot. Använd
    /// ``load(path:)`` när filen finns på disk.
    public init(text: String) {
        var out: [Entry] = []
        SSHConfig.collectEntries(text: text, baseDir: nil, depth: 0, into: &out)
        self.entries = out
    }

    /// Läser en config från disk och FÖLJER `Include`-rader.
    ///
    /// Det här är skillnaden mellan att läsa en modern `~/.ssh/config` och att
    /// läsa ingenting alls: `Include ~/.ssh/config.d/*` är hur de flesta
    /// verktyg (1Password, Colima, OrbStack m.fl.) säger åt användaren att
    /// lägga upp sin config, och då står det inte en enda `Host`-rad i
    /// huvudfilen.
    public static func load(path: String = SSHConfig.defaultPath) -> SSHConfig {
        guard let text = try? String(contentsOfFile: path, encoding: .utf8) else {
            return SSHConfig(entries: [])
        }
        var out: [Entry] = []
        collectEntries(
            text: text, baseDir: (path as NSString).deletingLastPathComponent,
            depth: 0, into: &out)
        return SSHConfig(entries: out)
    }

    /// Tolkar en config-text till poster, och expanderar `Include` INLINE på
    /// den plats raden stod. Att inlina är inte en förenkling utan just vad
    /// OpenSSH gör: en inkluderad fils `Host`-block gäller vidare efter
    /// include-punkten, precis som om innehållet stått där direkt.
    private static func collectEntries(
        text: String, baseDir: String?, depth: Int, into out: inout [Entry]
    ) {
        for rawLine in text.split(whereSeparator: { $0 == "\n" || $0 == "\r" }) {
            guard let (key, value) = SSHConfig.tokenize(String(rawLine)) else { continue }
            switch key {
            case "host":
                out.append(.host(value.split(whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)))
            case "match":
                out.append(.match(value))
            case "include":
                guard let baseDir, depth < maxIncludeDepth else { continue }
                for included in resolveInclude(value, baseDir: baseDir) {
                    guard let nested = try? String(contentsOfFile: included, encoding: .utf8)
                    else { continue }
                    collectEntries(text: nested, baseDir: baseDir, depth: depth + 1, into: &out)
                }
            default:
                out.append(.setting(key, value))
            }
        }
    }

    /// Löser upp en `Include`-rads sökvägar till konkreta filer.
    ///
    /// En rad kan ange flera sökvägar separerade med blanksteg, var och en med
    /// `~` och/eller jokertecken. Relativa sökvägar räknas från katalogen
    /// configfilen ligger i — OpenSSH säger `~/.ssh` för användarens config,
    /// vilket är samma katalog i praktiken men blir rätt även när filen ligger
    /// någon annanstans (t.ex. i ett test).
    ///
    /// Träffarna sorteras. OpenSSH läser glob-träffar i den ordning `glob(3)`
    /// ger, alltså sorterad — och ordningen spelar roll, eftersom första
    /// värdet vinner per nyckel. En saknad fil hoppas över i stället för att
    /// fela som OpenSSH gör: alternativet vore att en enda kvarglömd rad
    /// efter ett avinstallerat verktyg gör att INGA värdar alls läses.
    private static func resolveInclude(_ value: String, baseDir: String) -> [String] {
        var out: [String] = []
        for raw in value.split(whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init) {
            let expanded = (raw as NSString).expandingTildeInPath
            let full = (expanded as NSString).isAbsolutePath
                ? expanded
                : (baseDir as NSString).appendingPathComponent(expanded)

            guard expanded.contains("*") || expanded.contains("?") else {
                out.append(full)
                continue
            }

            // Jokertecken hanteras bara i SISTA segmentet, som i OpenSSH:s egna
            // exempel (`Include ~/.ssh/config.d/*`). Ett mönster mitt i
            // sökvägen är sällsynt nog att inte vara värt en egen
            // katalogtraversering.
            let dir = (full as NSString).deletingLastPathComponent
            let pattern = (full as NSString).lastPathComponent
            guard let names = try? FileManager.default.contentsOfDirectory(atPath: dir) else {
                continue
            }
            out += names
                .filter { glob(pattern, $0) }
                .sorted()
                .map { (dir as NSString).appendingPathComponent($0) }
        }
        return out
    }

    /// Konkreta värdalias (inte jokertecken/negation) i den ordning de står —
    /// underlag för att importera värdar till host-databasen.
    public var hostAliases: [String] {
        var seen = Set<String>()
        var out: [String] = []
        for entry in entries {
            guard case .host(let patterns) = entry else { continue }
            for p in patterns where !p.contains("*") && !p.contains("?") && !p.hasPrefix("!") {
                if seen.insert(p).inserted { out.append(p) }
            }
        }
        return out
    }

    /// Slår upp ett alias. Nycklar före första `Host` är globala (gäller alla).
    public func resolve(_ alias: String) -> ResolvedHost {
        var found: [String: String] = [:]
        var active = true  // global sektion tills första Host/Match
        for entry in entries {
            switch entry {
            case .host(let patterns):
                active = SSHConfig.hostMatches(patterns, alias)
            case .match(let criteria):
                active = SSHConfig.matchIsActive(criteria, alias)
            case .setting(let key, let value):
                if active, found[key] == nil { found[key] = value }
            }
        }
        return ResolvedHost(
            hostName: found["hostname"] ?? alias,
            user: found["user"],
            port: found["port"].flatMap { Int($0) } ?? 22,
            identityFile: found["identityfile"].map {
                ($0 as NSString).expandingTildeInPath
            },
            proxyJump: found["proxyjump"],
            forwardAgent: found["forwardagent"]?.lowercased() == "yes",
            remoteCommand: found["remotecommand"])
    }

    // MARK: - Parsning

    /// Delar en rad i (nyckel-gemener, värde). Stöder `Key Value`, `Key=Value`,
    /// `Key = Value` och citerade värden. Returnerar nil för tomma/kommentarrader.
    static func tokenize(_ line: String) -> (String, String)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty || trimmed.hasPrefix("#") { return nil }
        guard let sep = trimmed.firstIndex(where: { $0 == " " || $0 == "\t" || $0 == "=" }) else {
            return (trimmed.lowercased(), "")
        }
        let key = String(trimmed[..<sep]).lowercased()
        var value = String(trimmed[trimmed.index(after: sep)...])
            .trimmingCharacters(in: CharacterSet(charactersIn: " \t="))
        if value.count >= 2, value.hasPrefix("\""), value.hasSuffix("\"") {
            value = String(value.dropFirst().dropLast())
        }
        return (key, value)
    }

    /// En värd matchar om minst ett positivt mönster matchar och inget negerat gör det.
    /// Plockar ut värdaliaset ur ett `ProxyJump`-värde.
    ///
    /// Syntaxen är `[user@]host[:port]`, och flera hopp kan anges
    /// kommaseparerat. Bara FÖRSTA hoppet returneras — anslutningskedjan
    /// stöder ändå bara ett hopp, så att importera en längre kedja skulle
    /// skapa en koppling som inte går att använda.
    ///
    /// `nil` när värdet är tomt eller `none` (OpenSSH:s sätt att stänga av
    /// ett ärvt `ProxyJump`).
    public static func proxyJumpAlias(_ value: String) -> String? {
        guard let first = value.split(separator: ",").first.map(String.init)?
            .trimmingCharacters(in: .whitespaces), !first.isEmpty else { return nil }
        if first.lowercased() == "none" { return nil }
        let withoutUser = first.split(separator: "@").last.map(String.init) ?? first
        // IPv6-literaler skrivs `[::1]:22` — allt före den avslutande
        // klammern hör till adressen, inte till porten.
        let host: String
        if let end = withoutUser.firstIndex(of: "]") {
            host = String(withoutUser[...end])
        } else {
            host = withoutUser.split(separator: ":").first.map(String.init) ?? withoutUser
        }
        return host.isEmpty ? nil : host
    }

    /// Avgör om ett `Match`-blocks kriterier gäller för `alias`.
    ///
    /// OpenSSH kräver att ALLA kriterier på raden är uppfyllda. Här kan bara
    /// två av dem avgöras: `all` (alltid) och `host <mönster>` (samma
    /// jokertecken- och negationsregler som `Host`). Resten — `exec`, `user`,
    /// `originalhost`, `localuser`, `tagged`, `final`, `canonical` — beror på
    /// en pågående anslutning, en kommandokörning eller en andra
    /// upplösningsomgång, inget av det finns här.
    ///
    /// Ett okänt eller oavgörbart kriterium gör blocket INAKTIVT, aldrig
    /// aktivt. Det är den enda riktning som är säker: ett block som felaktigt
    /// hoppas över ger samma resultat som innan `Match` stöddes alls, medan
    /// ett block som felaktigt aktiveras tyst byter ut användarens värdnamn,
    /// användare eller nyckel mot någon annans.
    ///
    /// `Match exec "..."` kommer aldrig att köras härifrån. Att köra ett
    /// godtyckligt skalkommando för att avgöra en konfigurationsrad är inte
    /// en funktion som saknas, det är en vi inte vill ha.
    static func matchIsActive(_ criteria: String, _ alias: String) -> Bool {
        // Delas BARA på blanksteg. Ett `host`-kriterium tar en kommaseparerad
        // mönsterlista (`Match host *.internal,!hemlig.internal`); delas
        // kommatecknen redan här blir listans andra mönster ett eget, okänt
        // kriterium — vilket tyst gör varje negation till en avaktivering av
        // hela blocket. (Exakt den buggen fanns i Rust-porten först, och
        // fångades av dess negationstest.)
        let tokens = criteria
            .split(whereSeparator: { $0 == " " || $0 == "\t" })
            .map(String.init)
        guard !tokens.isEmpty else { return false }

        var i = 0
        var matchedSomething = false
        while i < tokens.count {
            switch tokens[i].lowercased() {
            case "all":
                matchedSomething = true
                i += 1
            case "host":
                // Kriteriet tar ett argument. Saknas det är raden trasig.
                guard i + 1 < tokens.count else { return false }
                let patterns = tokens[i + 1].split(separator: ",").map(String.init)
                guard SSHConfig.hostMatches(patterns, alias) else { return false }
                matchedSomething = true
                i += 2
            default:
                // Vi kan inte avgöra det, alltså gäller blocket inte.
                return false
            }
        }
        return matchedSomething
    }

    static func hostMatches(_ patterns: [String], _ host: String) -> Bool {
        guard !patterns.isEmpty else { return false }
        var matched = false
        for pattern in patterns {
            if pattern.hasPrefix("!") {
                if glob(String(pattern.dropFirst()), host) { return false }
            } else if glob(pattern, host) {
                matched = true
            }
        }
        return matched
    }

    /// Jokertecken-matchning med `*` (noll+ tecken) och `?` (exakt ett tecken).
    static func glob(_ pattern: String, _ text: String) -> Bool {
        let p = Array(pattern), t = Array(text)
        var pi = 0, ti = 0
        var star = -1, mark = 0
        while ti < t.count {
            if pi < p.count, p[pi] == "?" || p[pi] == t[ti] {
                pi += 1; ti += 1
            } else if pi < p.count, p[pi] == "*" {
                star = pi; mark = ti; pi += 1
            } else if star != -1 {
                pi = star + 1; mark += 1; ti = mark
            } else {
                return false
            }
        }
        while pi < p.count, p[pi] == "*" { pi += 1 }
        return pi == p.count
    }
}
