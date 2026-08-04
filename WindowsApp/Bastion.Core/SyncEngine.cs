using System.Text.Json;

namespace Bastion.Core;

/// <summary>
/// Port av SyncEngine.merge — deterministisk last-write-wins-sammanslagning,
/// identisk med Swift och LinuxApp/src/sync.rs. Se SYNC_PROTOCOL.md i
/// repo-roten för den formella specifikationen.
/// </summary>
public static class SyncEngine
{
    /// <summary>
    /// Stabil, ordningsoberoende tiebreak-nyckel för två <see cref="Host"/>-
    /// värden med EXAKT samma <c>ModifiedAt</c> — jämförelsen bryr sig bara
    /// om att den ger SAMMA svar för samma par oavsett besöksordning, inte
    /// om vilken av dem som "objektivt" är bäst (det finns inget sådant på
    /// en äkta tidsstämpel-krock). Samma princip som
    /// <c>LinuxApp/src/sync.rs</c>s <c>tiebreak_key</c>.
    /// </summary>
    private static string TiebreakKey(Host h) => JsonSerializer.Serialize(h);

    public static SyncState Merge(SyncState a, SyncState b)
    {
        var newestHost = new Dictionary<Guid, Host>();
        foreach (var h in a.Hosts.Concat(b.Hosts))
        {
            if (!newestHost.TryGetValue(h.Id, out var existing))
            {
                newestHost[h.Id] = h;
                continue;
            }
            // `>=` gjorde detta ORDNINGSBEROENDE på en EXAKT tidsstämpel-
            // krock: sist besökt i `Concat` (alltså `b`s kopia i
            // `Merge(a, b)`, men `a`s i `Merge(b, a)`) vann alltid —
            // `Merge(a, b) != Merge(b, a)`, ett brott mot kommutativitets-
            // löftet (CodeRabbit-fynd). Avgörs nu istället av en stabil
            // jämförelse av VÄRDET självt när tidsstämplarna är exakt lika.
            var cmp = h.ModifiedAt.Seconds.CompareTo(existing.ModifiedAt.Seconds);
            var replace = cmp > 0 || (cmp == 0 && string.CompareOrdinal(TiebreakKey(h), TiebreakKey(existing)) > 0);
            if (replace) newestHost[h.Id] = h;
        }

        var tomb = new Dictionary<Guid, ReferenceDate>();
        foreach (var (id, t) in a.Tombstones.Concat(b.Tombstones))
        {
            if (!tomb.TryGetValue(id, out var existing) || t.Seconds > existing.Seconds)
                tomb[id] = t;
        }

        var liveHosts = new List<Host>();
        var finalTombstones = new Dictionary<Guid, ReferenceDate>();
        var allIds = newestHost.Keys.Union(tomb.Keys);
        foreach (var id in allIds)
        {
            var hasHost = newestHost.TryGetValue(id, out var host);
            var hasTomb = tomb.TryGetValue(id, out var deletedAt);
            if (hasHost && hasTomb)
            {
                if (deletedAt.Seconds >= host!.ModifiedAt.Seconds) finalTombstones[id] = deletedAt;
                else liveHosts.Add(host);
            }
            else if (hasHost)
            {
                liveHosts.Add(host!);
            }
            else if (hasTomb)
            {
                finalTombstones[id] = deletedAt;
            }
        }

        return new SyncState
        {
            Hosts = liveHosts.OrderBy(h => h.Alias, StringComparer.OrdinalIgnoreCase).ToList(),
            Tombstones = finalTombstones,
        };
    }
}
