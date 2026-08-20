import Foundation

/// Ett synkbart tillstånd: levande värdar + gravstenar (raderade id:n med tid).
/// Detta är vad som skrivs till/läses från en synktransport (iCloud/Git/WebDAV/
/// mapp). Formatet är avsiktligt enkelt och diff-bart.
public struct SyncState: Codable, Sendable, Equatable {
    public var hosts: [Host]
    /// Synkade snippets. Tillkom efter att formatet fanns i skarp drift, så
    /// `decodeIfPresent` — ett tillstånd skrivet innan snippets ingick saknar
    /// fältet helt och ska läsas som "inga snippets", inte avvisas. Annars
    /// slutar synken fungera för alla som inte uppgraderat varje enhet
    /// samtidigt, vilket ingen gör.
    public var snippets: [Snippet]
    /// Delas av BÅDA posttyperna. UUID:n krockar inte mellan typerna, och en
    /// gemensam karta slipper ett andra fält på tråden.
    public var tombstones: [UUID: Date]

    public init(hosts: [Host] = [], snippets: [Snippet] = [], tombstones: [UUID: Date] = [:]) {
        self.hosts = hosts
        self.snippets = snippets
        self.tombstones = tombstones
    }

    private enum CodingKeys: String, CodingKey {
        case hosts, snippets, tombstones
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        hosts = try c.decode([Host].self, forKey: .hosts)
        snippets = try c.decodeIfPresent([Snippet].self, forKey: .snippets) ?? []
        tombstones = try c.decode([UUID: Date].self, forKey: .tombstones)
    }
}

/// Det en post måste kunna svara på för att kunna slås ihop.
///
/// Utbruten när snippets tillkom i synken. Att kopiera hopslagningen per typ
/// vore inte bara upprepning — reglerna (kommutativitet, gravstenar,
/// tidsstämpel-krockar) är subtila nog att en kopia förr eller senare hade
/// fått en av dem fel, och just den sortens fel yttrar sig som tappad
/// användardata utan felmeddelande.
protocol Mergeable: Codable {
    var mergeID: UUID { get }
    var mergeModifiedAt: Date { get }
}

extension Mergeable {
    /// Stabil, ordningsoberoende nyckel för två poster med EXAKT samma
    /// `modifiedAt`. Kravet är bara att jämförelsen ger SAMMA svar för samma
    /// par oavsett i vilken ordning paret besöks — det finns ingen post som
    /// "objektivt" är bäst på en äkta tidsstämpel-krock.
    ///
    /// `sortedKeys` gör kodningen deterministisk. Exakt samma sträng som
    /// LinuxApp bildar är däremot INTE garanterad, så de två plattformarna
    /// kan i teorin välja olika vinnare på en krock. Det kräver att två
    /// enheter redigerat samma post på exakt samma flyttalssekund, och nästa
    /// redigering på endera sidan löser upp det — priset för att undvika det
    /// helt vore en egen kanonisk serialisering på båda sidor, vilket kostar
    /// mer än det skyddar mot.
    var mergeTiebreakKey: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        return (try? encoder.encode(self)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
    }
}

extension Host: Mergeable {
    var mergeID: UUID { id }
    var mergeModifiedAt: Date { modifiedAt }
}

extension Snippet: Mergeable {
    var mergeID: UUID { id }
    var mergeModifiedAt: Date { modifiedAt }
}

/// Slår ihop två tillstånd deterministiskt utan server. Regler:
/// - Samma värd på båda sidor: nyaste `modifiedAt` vinner (last-write-wins).
/// - Radering (gravsten) vinner om den är minst lika ny som värdens ändring;
///   annars "återupplivas" värden (en nyare redigering slår en äldre radering).
/// - Resultatet är kommutativt och idempotent → säkert att köra upprepat och i
///   valfri ordning mellan enheter.
public enum SyncEngine {
    /// Behåller den nyaste kopian av varje id ur två listor.
    private static func newestByID<T: Mergeable>(_ a: [T], _ b: [T]) -> [UUID: T] {
        var newest: [UUID: T] = [:]
        for item in a + b {
            guard let existing = newest[item.mergeID] else {
                newest[item.mergeID] = item
                continue
            }
            // `>=` gjorde detta ORDNINGSBEROENDE på en EXAKT
            // tidsstämpel-krock: sist sedd i kedjan (alltså `b`s kopia i
            // merge(a, b), men `a`s i merge(b, a)) vann alltid, så
            // merge(a, b) != merge(b, a) för just det fallet — ett brott mot
            // kommutativitetslöftet nedan. Samma bugg fanns i LinuxApp och i
            // WindowsApp/Bastion.Core/SyncEngine.cs och rättades där; den här
            // sidan hade den kvar. Vid en RIKTIG krock avgörs det nu av en
            // stabil, ordningsoberoende jämförelse av VÄRDET självt.
            if item.mergeModifiedAt > existing.mergeModifiedAt
                || (item.mergeModifiedAt == existing.mergeModifiedAt
                    && item.mergeTiebreakKey > existing.mergeTiebreakKey)
            {
                newest[item.mergeID] = item
            }
        }
        return newest
    }

    /// Posterna som överlever gravstenarna. En gravsten vinner om den är minst
    /// lika ny som postens ändring; en NYARE ändring återupplivar posten.
    private static func survivors<T: Mergeable>(_ newest: [UUID: T], _ tomb: [UUID: Date]) -> [T] {
        newest.values.filter { item in
            guard let deletedAt = tomb[item.mergeID] else { return true }
            return deletedAt < item.mergeModifiedAt
        }
    }

    public static func merge(_ a: SyncState, _ b: SyncState) -> SyncState {
        let newestHosts = newestByID(a.hosts, b.hosts)
        let newestSnippets = newestByID(a.snippets, b.snippets)

        // Nyaste gravstenen per id.
        var tomb: [UUID: Date] = [:]
        for (id, t) in a.tombstones { tomb[id] = max(t, tomb[id] ?? .distantPast) }
        for (id, t) in b.tombstones { tomb[id] = max(t, tomb[id] ?? .distantPast) }

        // En gravsten faller bara om en NYARE post med samma id lever — och
        // den posten kan vara av vilken typ som helst, eftersom gravstenarna
        // delar karta. Att i stället behålla bara de gravstenar som saknar
        // levande VÄRD skulle tyst kasta varje gravsten som hörde till en
        // snippet, och då återuppstår raderade snippets vid nästa synk.
        var finalTombstones: [UUID: Date] = [:]
        for (id, deletedAt) in tomb {
            let revivedHost = (newestHosts[id]?.mergeModifiedAt).map { $0 > deletedAt } ?? false
            let revivedSnippet =
                (newestSnippets[id]?.mergeModifiedAt).map { $0 > deletedAt } ?? false
            if !revivedHost && !revivedSnippet { finalTombstones[id] = deletedAt }
        }

        return SyncState(
            hosts: survivors(newestHosts, tomb)
                .sorted { $0.alias.lowercased() < $1.alias.lowercased() },
            snippets: survivors(newestSnippets, tomb)
                .sorted { $0.name.lowercased() < $1.name.lowercased() },
            tombstones: finalTombstones)
    }
}
