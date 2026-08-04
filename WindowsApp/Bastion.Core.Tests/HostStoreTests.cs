using Bastion.Core;

namespace Bastion.Core.Tests;

public class HostStoreTests
{
    private static string TempDir() => Path.Combine(Path.GetTempPath(), $"bastion-cs-test-{Guid.NewGuid()}");

    [Fact]
    public void HostStoreRoundTripsThroughDisk()
    {
        var dir = TempDir();
        var path = Path.Combine(dir, "hosts.json");
        var store = new HostStore(path);
        var host = Host.Create("mp100", "192.168.1.50", "berduf");
        var id = host.Id;
        store.Upsert(host);

        var reopened = new HostStore(path);
        var all = reopened.All();
        Assert.Single(all);
        Assert.Equal(id, all[0].Id);
        Directory.Delete(dir, true);
    }

    /// <summary>
    /// Läser en riktig hosts.json genererad av en verklig `swift`-körning
    /// (fixturen kopieras in i testutdatakatalogen, se .csproj) — verifierar
    /// att C#-sidan verkligen kan avkoda Swifts faktiska wire-format, inte
    /// bara ett antaget sådant.
    /// </summary>
    [Fact]
    public void ReadsAHostsJsonActuallyProducedBySwift()
    {
        var fixturePath = Path.Combine(AppContext.BaseDirectory, "csharp-hosts-fixture.json");
        Assert.True(File.Exists(fixturePath), $"Fixture saknas: {fixturePath}");

        var store = new HostStore(fixturePath);
        var hosts = store.All();
        Assert.Single(hosts);
        Assert.Equal("csharp-check", hosts[0].Alias);
        Assert.Equal("10.0.0.9", hosts[0].HostName);
        Assert.Equal(new HostAuth.KeyFile("/home/x/.ssh/id_ed25519"), hosts[0].Auth);
    }

    [Fact]
    public void ACorruptHostsJsonThrowsInsteadOfSilentlyBecomingEmpty()
    {
        var dir = TempDir();
        Directory.CreateDirectory(dir);
        var path = Path.Combine(dir, "hosts.json");
        File.WriteAllText(path, "{ det här är inte giltig json");

        Assert.ThrowsAny<System.Text.Json.JsonException>(() => new HostStore(path));
        Directory.Delete(dir, true);
    }

    /// <summary>
    /// Ett äldre format (en ren <c>Host[]</c>-array utan SyncState-omslag)
    /// ska fortfarande gå att läsa — regressionstest för
    /// <see cref="SyncStateConverter"/>s omskrivning (den skulle annars
    /// kunna kasta ett otypat undantag på just den här formen och missa
    /// fallback-vägen helt).
    /// </summary>
    [Fact]
    public void ALegacyBareHostArrayStillLoadsCorrectly()
    {
        var dir = TempDir();
        Directory.CreateDirectory(dir);
        var path = Path.Combine(dir, "hosts.json");
        File.WriteAllText(path, """[{"id":"00000000-0000-0000-0000-000000000001","alias":"legacy","hostName":"h","user":"u","modifiedAt":0}]""");

        var store = new HostStore(path);
        var hosts = store.All();
        Assert.Single(hosts);
        Assert.Equal("legacy", hosts[0].Alias);
        Directory.Delete(dir, true);
    }

    [Fact]
    public void DeleteAddsATombstoneThatSurvivesReopen()
    {
        var dir = TempDir();
        var path = Path.Combine(dir, "hosts.json");
        var store = new HostStore(path);
        var host = Host.Create("t", "h", "u");
        store.Upsert(host);
        store.Delete(host.Id);

        Assert.Empty(store.All());
        var reopened = new HostStore(path);
        Assert.Empty(reopened.All());
        Directory.Delete(dir, true);
    }
}

public class SyncEngineTests
{
    private static Host HostAt(string alias, Guid id, double modifiedAt) => new()
    {
        Id = id,
        Alias = alias,
        HostName = "h",
        User = "u",
        ModifiedAt = new ReferenceDate(modifiedAt),
    };

    [Fact]
    public void NewerEditWinsOverOlderEdit()
    {
        var id = Guid.NewGuid();
        var a = new SyncState { Hosts = { HostAt("gammal", id, 10) } };
        var b = new SyncState { Hosts = { HostAt("ny", id, 20) } };
        var merged = SyncEngine.Merge(a, b);
        Assert.Single(merged.Hosts);
        Assert.Equal("ny", merged.Hosts[0].Alias);
    }

    [Fact]
    public void TombstoneWinsWhenNewerThanEdit()
    {
        var id = Guid.NewGuid();
        var a = new SyncState { Hosts = { HostAt("host", id, 10) } };
        var b = new SyncState { Tombstones = { [id] = new ReferenceDate(20) } };
        var merged = SyncEngine.Merge(a, b);
        Assert.Empty(merged.Hosts);
        Assert.Single(merged.Tombstones);
    }

    [Fact]
    public void NewerEditRevivesOverOlderTombstone()
    {
        var id = Guid.NewGuid();
        var a = new SyncState { Tombstones = { [id] = new ReferenceDate(10) } };
        var b = new SyncState { Hosts = { HostAt("återupplivad", id, 20) } };
        var merged = SyncEngine.Merge(a, b);
        Assert.Single(merged.Hosts);
        Assert.Equal("återupplivad", merged.Hosts[0].Alias);
        Assert.Empty(merged.Tombstones);
    }

    /// <summary>
    /// Regressionstest för en ORDNINGSBEROENDE tie-bugg (CodeRabbit-fynd):
    /// på en EXAKT tidsstämpel-krock vann tidigare alltid den sist besökta
    /// kopian i <c>Concat</c> — <c>Merge(a, b)</c> gav ett annat resultat
    /// än <c>Merge(b, a)</c>.
    /// </summary>
    [Fact]
    public void MergeIsCommutativeEvenOnAnExactModifiedAtTie()
    {
        var id = Guid.NewGuid();
        var a = new SyncState { Hosts = { HostAt("alpha", id, 42) } };
        var b = new SyncState { Hosts = { HostAt("bravo", id, 42) } };
        var ab = SyncEngine.Merge(a, b);
        var ba = SyncEngine.Merge(b, a);
        Assert.Single(ab.Hosts);
        Assert.Equal(ab.Hosts[0].Alias, ba.Hosts[0].Alias);
    }

    /// <summary>
    /// Riktig cross-instans-verifiering: två oberoende HostStores synkar via
    /// en delad FolderSyncProvider-fil och KONVERGERAR — samma test som
    /// LinuxApp/src/sync.rs, nu i C# för att bevisa att protokollet
    /// verkligen är klientoberoende (tredje språket).
    /// </summary>
    [Fact]
    public void TwoIndependentStoresConvergeThroughASharedFolderProvider()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-cs-sync-test-{Guid.NewGuid()}");
        var storeAPath = Path.Combine(dir, "a", "hosts.json");
        var storeBPath = Path.Combine(dir, "b", "hosts.json");
        var sharedPath = Path.Combine(dir, "shared", "hosts.json");

        var storeA = new HostStore(storeAPath);
        var storeB = new HostStore(storeBPath);

        storeA.Upsert(Host.Create("från-a", "1.2.3.4", "u"));
        var provider = new FolderSyncProvider(sharedPath);
        storeA.Sync(provider);

        storeB.Upsert(Host.Create("från-b", "5.6.7.8", "u"));
        storeB.Sync(provider);

        storeA.Sync(provider);

        var aliasesA = storeA.All().Select(h => h.Alias).ToList();
        Assert.Contains("från-a", aliasesA);
        Assert.Contains("från-b", aliasesA);

        Directory.Delete(dir, true);
    }
}
