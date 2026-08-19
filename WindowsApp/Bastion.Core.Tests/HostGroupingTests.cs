using Bastion.Core;

namespace Bastion.Core.Tests;

/// <summary>
/// Samma fall som LinuxApp/src/host_grouping.rs testar — grupperingen måste
/// se likadan ut på alla plattformar, annars hamnar samma värd i olika
/// sektioner beroende på vilken klient man tittar i.
/// </summary>
public class HostGroupingTests
{
    private static Host MakeHost(string alias, string[] tags, bool favorite = false) => new()
    {
        Alias = alias,
        HostName = $"{alias}.example.invalid",
        User = "user",
        Tags = [.. tags],
        IsFavorite = favorite,
    };

    [Fact]
    public void FavoritesGetTheirOwnSectionFirstRegardlessOfTags()
    {
        var groups = HostGrouping.Grouped([
            MakeHost("b-server", ["prod"]),
            MakeHost("a-favorit", ["prod"], favorite: true),
        ]);

        Assert.Equal(HostGrouping.FavoritesTag, groups[0].Tag);
        Assert.Equal("a-favorit", Assert.Single(groups[0].Hosts).Alias);
        Assert.Equal("prod", groups[1].Tag);
        Assert.Equal("b-server", Assert.Single(groups[1].Hosts).Alias);
    }

    [Fact]
    public void FavoriteSectionIsAbsentWhenNoHostIsFavorite()
    {
        var groups = HostGrouping.Grouped([MakeHost("web", ["prod"])]);

        Assert.DoesNotContain(groups, g => g.Tag == HostGrouping.FavoritesTag);
    }

    [Fact]
    public void UntaggedHostsLandInOvriga()
    {
        var groups = HostGrouping.Grouped([MakeHost("lös", [])]);

        Assert.Equal(HostGrouping.UntaggedTag, Assert.Single(groups).Tag);
    }

    [Fact]
    public void HostWithSeveralTagsAppearsInEachSection()
    {
        var groups = HostGrouping.Grouped([MakeHost("nas", ["homelab", "lagring"])]);

        Assert.Equal(["homelab", "lagring"], groups.Select(g => g.Tag));
        Assert.All(groups, g => Assert.Equal("nas", Assert.Single(g.Hosts).Alias));
    }

    [Fact]
    public void TagsAndAliasesSortCaseInsensitively()
    {
        var groups = HostGrouping.Grouped([
            MakeHost("Zeta", ["Prod"]),
            MakeHost("alfa", ["prod"]),
            MakeHost("beta", ["Kunder"]),
        ]);

        Assert.Equal(["Kunder", "Prod", "prod"], groups.Select(g => g.Tag));
        Assert.Equal("Zeta", Assert.Single(groups[1].Hosts).Alias);
        Assert.Equal("alfa", Assert.Single(groups[2].Hosts).Alias);
    }

    [Fact]
    public void FilterMatchesAliasHostNameUserAndTag()
    {
        var hosts = new[]
        {
            MakeHost("web-01", ["prod"]),
            MakeHost("db-01", ["prod"]),
            MakeHost("plex", ["homelab"]),
        };

        Assert.Equal("web-01", OnlyHost(HostGrouping.GroupedAndFiltered(hosts, "web")));
        Assert.Equal("plex", OnlyHost(HostGrouping.GroupedAndFiltered(hosts, "HOMELAB")));
        Assert.Equal("db-01", OnlyHost(HostGrouping.GroupedAndFiltered(hosts, "db-01.example")));
        Assert.Equal(3, HostGrouping.GroupedAndFiltered(hosts, "user").Sum(g => g.Hosts.Count));
    }

    [Fact]
    public void EmptySectionsDisappearAndEmptyQueryKeepsEverything()
    {
        var hosts = new[] { MakeHost("web-01", ["prod"]), MakeHost("plex", ["homelab"]) };

        var filtered = HostGrouping.GroupedAndFiltered(hosts, "plex");
        Assert.Equal("homelab", Assert.Single(filtered).Tag);

        Assert.Equal(2, HostGrouping.GroupedAndFiltered(hosts, "   ").Count);
    }

    private static string OnlyHost(IReadOnlyList<HostGroup> groups) =>
        Assert.Single(Assert.Single(groups).Hosts).Alias;
}
