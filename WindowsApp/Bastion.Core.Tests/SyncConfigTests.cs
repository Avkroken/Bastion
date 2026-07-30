using Bastion.Core;

namespace Bastion.Core.Tests;

public class SyncConfigTests
{
    [Fact]
    public void RoundTripsThroughDisk()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-cs-syncconfig-{Guid.NewGuid()}");
        var path = Path.Combine(dir, "sync-config.json");
        var config = new SyncConfig { FolderPath = @"D:\Syncthing\bastion" };
        config.Save(path);

        var reloaded = SyncConfig.Load(path);
        Assert.Equal(@"D:\Syncthing\bastion", reloaded.FolderPath);
        Directory.Delete(dir, true);
    }

    [Fact]
    public void MissingFileLoadsAsEmptyConfig()
    {
        var config = SyncConfig.Load(Path.Combine(Path.GetTempPath(), $"bastion-nonexistent-{Guid.NewGuid()}.json"));
        Assert.Null(config.FolderPath);
    }
}
