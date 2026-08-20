using Bastion.Core;

namespace Bastion.Core.Tests;

/// <summary>
/// Samma fixtur och samma påståenden som Tests/SSHCoreTests/SystemProbeTests.swift
/// — porten ska ge identiskt resultat, annars visar Windows andra siffror än
/// iOS/macOS för samma värd.
/// </summary>
public class SystemProbeTests
{
    // Fixtur byggd på verklig utdata från en Ubuntu-maskin.
    private const string Fixture = """
        @@LOADAVG
        2.53 1.86 1.81 3/1005 1015672
        @@UPTIME
        259335.25 2634310.61
        @@MEM
        MemTotal:       15244848 kB
        MemFree:          680100 kB
        MemAvailable:   10814800 kB
        @@DF
        Filesystem          1024-blocks       Used   Available Capacity Mounted on
        tmpfs                   3048972       2472     3046500       1% /run
        /dev/nvme0n1p2        102626232   22509936    74857032      24% /
        tmpfs                   7622424          0     7622424       0% /dev/shm
        @@OS
        PRETTY_NAME="Ubuntu 26.04 LTS"
        NAME="Ubuntu"
        @@KERNEL
        Linux 7.0.0-27-generic
        @@HOST
        mp100
        @@NPROC
        12
        @@DOCKER
        a1b2c3d4e5f6|plex|linuxserver/plex:latest|Up 3 days
        f6e5d4c3b2a1|radarr|linuxserver/radarr|Up 2 hours (healthy)
        @@END
        """;

    [Fact]
    public void ParsesFullSnapshot()
    {
        var s = SystemProbe.Parse(Fixture);

        Assert.Equal(new LoadAverage(2.53, 1.86, 1.81), s.Load);
        Assert.Equal(259335.25, s.UptimeSeconds);
        Assert.Equal("Linux 7.0.0-27-generic", s.Kernel);
        Assert.Equal("mp100", s.Hostname);
        Assert.Equal("Ubuntu 26.04 LTS", s.Os);
        Assert.Equal(12, s.CpuCount);

        Assert.Equal(15244848L * 1024, s.Memory!.TotalBytes);
        Assert.Equal(10814800L * 1024, s.Memory.AvailableBytes);
        Assert.Equal((15244848L - 10814800) * 1024, s.Memory.UsedBytes);

        // Rot-disken plockas ut korrekt bland flera monteringar.
        Assert.Equal(3, s.Disks.Count);
        var root = s.RootDisk;
        Assert.Equal("/dev/nvme0n1p2", root!.Filesystem);
        Assert.Equal(24, root.CapacityPercent);
        Assert.Equal(102626232L * 1024, root.SizeBytes);

        Assert.Equal(2, s.Containers.Count);
        Assert.Equal("plex", s.Containers[0].Name);
        Assert.Equal("Up 2 hours (healthy)", s.Containers[^1].Status);
        Assert.True(s.Containers[0].IsRunning);
    }

    [Fact]
    public void MissingSectionsAreNullNotCrash()
    {
        var s = SystemProbe.Parse("@@LOADAVG\n0.10 0.20 0.30\n@@END");

        Assert.Equal(new LoadAverage(0.10, 0.20, 0.30), s.Load);
        Assert.Null(s.Memory);
        Assert.Null(s.Os);
        Assert.Null(s.Kernel);
        Assert.Null(s.CpuCount);
        Assert.Null(s.UptimeSeconds);
        Assert.Empty(s.Disks);
        Assert.Empty(s.Containers);
        Assert.Null(s.RootDisk);
    }

    [Fact]
    public void GarbageOutputYieldsEmptySnapshot()
    {
        var s = SystemProbe.Parse("bash: nproc: command not found\nsegmentation fault\n");

        Assert.Null(s.Load);
        Assert.Null(s.Memory);
        Assert.Empty(s.Disks);
        Assert.Empty(s.Containers);
    }

    [Fact]
    public void MemoryWithoutMemAvailableIsNullRatherThanZero()
    {
        var s = SystemProbe.Parse("@@MEM\nMemTotal:       15244848 kB\nMemFree:          680100 kB\n@@END");

        Assert.Null(s.Memory);
    }

    [Fact]
    public void DiskUsageIgnoresRubbishRowsButKeepsMountsWithSpaces()
    {
        var s = SystemProbe.Parse("""
            @@DF
            Filesystem          1024-blocks       Used   Available Capacity Mounted on
            df: /mnt/trasig: Input/output error
            /dev/sdb1              1048576     524288      524288      50% /mnt/mina filer
            @@END
            """);

        var disk = Assert.Single(s.Disks);
        Assert.Equal("/mnt/mina filer", disk.Mount);
        Assert.Equal(50, disk.CapacityPercent);
        Assert.Equal(524288L * 1024, disk.UsedBytes);
    }

    [Fact]
    public void LoadAverageParsesWithInvariantCultureRegardlessOfThread()
    {
        var previous = Thread.CurrentThread.CurrentCulture;
        Thread.CurrentThread.CurrentCulture = new System.Globalization.CultureInfo("sv-SE");
        try
        {
            // Svenskt lokalformat använder komma som decimaltecken — värden
            // skickar alltid punkt, så parsningen får inte följa trådens kultur.
            var s = SystemProbe.Parse("@@LOADAVG\n2.53 1.86 1.81\n@@UPTIME\n259335.25 0.0\n@@END");

            Assert.Equal(new LoadAverage(2.53, 1.86, 1.81), s.Load);
            Assert.Equal(259335.25, s.UptimeSeconds);
        }
        finally
        {
            Thread.CurrentThread.CurrentCulture = previous;
        }
    }

    [Fact]
    public void CommandReadsOnlyAndSwallowsMissingTools()
    {
        // Proben körs mot främmande servrar: den ska aldrig skriva något, och
        // saknade verktyg ska inte spilla fel till användaren.
        Assert.DoesNotContain(">", SystemProbe.Command.Replace("2>/dev/null", ""));
        Assert.Contains("@@DOCKER", SystemProbe.Command);
        Assert.EndsWith("echo @@END", SystemProbe.Command);
    }
}

/// <summary>
/// Sektionerna som gör dashboarden komplett mot VISION (temperatur, IP-adresser,
/// SSH-nycklar, aktiva användare) — samma fall som LinuxApp/src/dashboard.rs testar.
/// </summary>
public class SystemProbeDashboardTests
{
    [Fact]
    public void ParsesEverySectionVisionAsksFor()
    {
        var s = SystemProbe.Parse("""
            @@HOST
            srv1
            @@TEMP
            x86_pkg_temp|55000
            @@IP
            2: eth0    inet 10.0.0.5/24 brd 10.0.0.255 scope global eth0
            @@KEYS
            256 SHA256:abc anders@laptop (ED25519)
            @@WHO
            anders   pts/0        2026-08-18 19:14 (10.0.0.1)
            @@END
            """);

        Assert.Equal("srv1", s.Hostname);
        Assert.Equal(new Temperature("x86_pkg_temp", 55), Assert.Single(s.Temperatures));
        Assert.Equal(new IpAddress("eth0", "10.0.0.5/24", false), Assert.Single(s.Addresses));
        Assert.Equal(new AuthorizedKey(256, "SHA256:abc", "anders@laptop", "ED25519"), Assert.Single(s.AuthorizedKeys));
        Assert.Equal(new ActiveUser("anders", "pts/0", "2026-08-18 19:14", "10.0.0.1"), Assert.Single(s.ActiveUsers));
    }

    [Fact]
    public void HostWithoutTheseToolsGivesEmptyListsNotErrors()
    {
        var s = SystemProbe.Parse("@@HOST\nsrv2\n@@END");

        Assert.Equal("srv2", s.Hostname);
        Assert.Empty(s.Temperatures);
        Assert.Empty(s.Addresses);
        Assert.Empty(s.AuthorizedKeys);
        Assert.Empty(s.ActiveUsers);
    }

    [Fact]
    public void MilligradesBecomeCelsiusAndBrokenSensorsAreDropped()
    {
        var s = SystemProbe.Parse("""
            @@TEMP
            x86_pkg_temp|55000
            |48500
            acpitz|-274000
            trasig|inte-ett-tal
            @@END
            """);

        Assert.Equal([55.0, 48.5], s.Temperatures.Select(t => t.Celsius));
        Assert.Equal("okänd", s.Temperatures[1].Label);
    }

    [Fact]
    public void FallbackAddressesAreUsedOnlyWhenIpGaveNothing()
    {
        var withIp = SystemProbe.Parse("""
            @@IP
            2: eth0    inet 10.0.0.5/24 scope global eth0
            @@IPFALLBACK
            192.168.1.10 fd00::1
            @@END
            """);
        Assert.Equal("eth0", Assert.Single(withIp.Addresses).Interface);

        var withoutIp = SystemProbe.Parse("@@IP\n@@IPFALLBACK\n192.168.1.10 fd00::1\n@@END");
        Assert.Equal(2, withoutIp.Addresses.Count);
        Assert.All(withoutIp.Addresses, a => Assert.Equal("okänt", a.Interface));
        // Utan prefixlängd avgörs familjen av kolon.
        Assert.False(withoutIp.Addresses[0].IsIpv6);
        Assert.True(withoutIp.Addresses[1].IsIpv6);
    }

    [Fact]
    public void KeyCommentsMayContainSpacesAndMayBeMissing()
    {
        var s = SystemProbe.Parse("""
            @@KEYS
            256 SHA256:abc anders laptop hemma (ED25519)
            4096 SHA256:def  (RSA)
            skräprad utan parentes
            @@END
            """);

        Assert.Equal("anders laptop hemma", s.AuthorizedKeys[0].Comment);
        Assert.Equal(4096, s.AuthorizedKeys[1].Bits);
        Assert.Equal("", s.AuthorizedKeys[1].Comment);
        Assert.Equal(2, s.AuthorizedKeys.Count);
    }

    [Fact]
    public void LocalLoginHasNoOrigin()
    {
        var s = SystemProbe.Parse("""
            @@WHO
            anders   tty1         2026-08-18 08:02
            drift    pts/2        2026-08-19 04:31 (10.0.0.1)
            @@END
            """);

        Assert.Null(s.ActiveUsers[0].From);
        Assert.Equal("10.0.0.1", s.ActiveUsers[1].From);
        Assert.Equal("2026-08-19 04:31", s.ActiveUsers[1].Since);
    }

    [Fact]
    public void CommandAsksForEverySectionTheParserReads()
    {
        foreach (var section in new[] { "@@TEMP", "@@IP", "@@IPFALLBACK", "@@KEYS", "@@WHO" })
        {
            Assert.Contains(section, SystemProbe.Command);
        }
    }
}
