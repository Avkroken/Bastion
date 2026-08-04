using System.Text.Json;
using Bastion.Core;
using Xunit;

namespace Bastion.Core.Tests;

public class AppSettingsTests
{
    [Fact]
    public void DefaultsAreAllTrueSoUpgradesDontLoseButtons()
    {
        var t = new FeatureToggles();
        Assert.True(t.ShowDocker && t.ShowSnippets && t.ShowCommandLibrary);
        Assert.True(t.ShowSftpBrowser && t.ShowPortForward && t.ShowKeyDeploy);
    }

    [Fact]
    public void RoundTripsThroughDisk()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-settings-test-{Guid.NewGuid()}");
        var path = Path.Combine(dir, "settings.json");
        try
        {
            var store = new AppSettingsStore(path);
            store.Update(store.Current() with { ShowDocker = false });

            var reopened = new AppSettingsStore(path);
            Assert.False(reopened.Current().ShowDocker);
        }
        finally
        {
            if (Directory.Exists(dir)) Directory.Delete(dir, recursive: true);
        }
    }

    [Fact]
    public void ACorruptSettingsFileThrowsInsteadOfSilentlyFallingBackToDefaults()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-settings-test-{Guid.NewGuid()}");
        Directory.CreateDirectory(dir);
        var path = Path.Combine(dir, "settings.json");
        try
        {
            File.WriteAllText(path, "{ inte giltig json");
            Assert.ThrowsAny<JsonException>(() => new AppSettingsStore(path));
        }
        finally
        {
            Directory.Delete(dir, recursive: true);
        }
    }

    [Fact]
    public void WireFormatMatchesARealSwiftEncoding()
    {
        // Verifierat mot en riktig `swift`-körning (samma struct, JSONEncoder), se settings.rs:
        // {"showCommandLibrary":true,"showDocker":true,"showKeyDeploy":true,
        //  "showPortForward":true,"showSFTPBrowser":true,"showSnippets":true}
        var json = JsonSerializer.Serialize(new FeatureToggles());
        Assert.Contains("\"showDocker\":true", json);
        Assert.Contains("\"showSFTPBrowser\":true", json);
        Assert.DoesNotContain("showSftpBrowser", json);
    }

    [Fact]
    public void AMissingFieldDefaultsToTrueNotADecodeError()
    {
        const string partial = """{"showDocker":false}""";
        var t = JsonSerializer.Deserialize<FeatureToggles>(partial)!;
        Assert.False(t.ShowDocker);
        Assert.True(t.ShowSnippets);
    }
}
