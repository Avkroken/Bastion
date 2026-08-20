namespace Bastion.Core;

/// <summary>En sektion i värdlistan: taggnamn + värdarna under den.</summary>
public sealed record HostGroup(string Tag, IReadOnlyList<Host> Hosts);

/// <summary>
/// Port av <c>HostListModel.groups</c> + <c>HostListView.filteredGroups</c> i
/// App/HostListView.swift, samma logik som LinuxApp/src/host_grouping.rs.
/// Ren (WinUI-fri) logik just för att kunna testas — XAML-limkoden verifieras
/// bara av ett lyckat bygge.
/// </summary>
public static class HostGrouping
{
    public const string FavoritesTag = "★ Favoriter";
    public const string UntaggedTag = "Övriga";

    /// <summary>
    /// Favoriter (oavsett tagg) i en egen sektion FÖRST — bara om den är
    /// icke-tom. Resten grupperas per tagg, alfabetiskt och skiftlägesokänsligt;
    /// en värd utan taggar hamnar under "Övriga", en värd med flera taggar
    /// förekommer i VARJE sin taggs sektion (matchar Swift- och Rust-sidan rakt
    /// av, inte en bugg). Varje sektion sorteras på alias, skiftlägesokänsligt.
    /// </summary>
    public static IReadOnlyList<HostGroup> Grouped(IEnumerable<Host> hosts)
    {
        var all = hosts.ToList();
        var byTag = new Dictionary<string, List<Host>>(StringComparer.Ordinal);
        foreach (var host in all.Where(h => !h.IsFavorite))
        {
            var tags = host.Tags.Count > 0 ? host.Tags : [UntaggedTag];
            foreach (var tag in tags)
            {
                if (!byTag.TryGetValue(tag, out var bucket)) byTag[tag] = bucket = [];
                bucket.Add(host);
            }
        }

        var groups = byTag.Keys
            .OrderBy(t => t.ToLowerInvariant(), StringComparer.Ordinal)
            .Select(t => new HostGroup(t, Sorted(byTag[t])))
            .ToList();

        var favorites = Sorted(all.Where(h => h.IsFavorite));
        if (favorites.Count > 0) groups.Insert(0, new HostGroup(FavoritesTag, favorites));
        return groups;
    }

    /// <summary>
    /// Filtrerar sektionerna på söktext — alias, värdnamn, användare och taggar,
    /// skiftlägesokänsligt. En sektion utan träffar faller bort helt, den döljs inte.
    /// </summary>
    public static IReadOnlyList<HostGroup> Filter(IEnumerable<HostGroup> groups, string query)
    {
        var needle = query.Trim().ToLowerInvariant();
        if (needle.Length == 0) return groups.ToList();

        return groups
            .Select(g => new HostGroup(g.Tag, g.Hosts.Where(h => Matches(h, needle)).ToList()))
            .Where(g => g.Hosts.Count > 0)
            .ToList();
    }

    /// <summary>Gruppering och filtrering i ett — det anropet UI:t gör.</summary>
    public static IReadOnlyList<HostGroup> GroupedAndFiltered(IEnumerable<Host> hosts, string query) =>
        Filter(Grouped(hosts), query);

    private static bool Matches(Host host, string needle) =>
        host.Alias.Contains(needle, StringComparison.OrdinalIgnoreCase)
        || host.HostName.Contains(needle, StringComparison.OrdinalIgnoreCase)
        || host.User.Contains(needle, StringComparison.OrdinalIgnoreCase)
        || host.Tags.Any(t => t.Contains(needle, StringComparison.OrdinalIgnoreCase));

    private static List<Host> Sorted(IEnumerable<Host> hosts) =>
        hosts.OrderBy(h => h.Alias.ToLowerInvariant(), StringComparer.Ordinal).ToList();
}
