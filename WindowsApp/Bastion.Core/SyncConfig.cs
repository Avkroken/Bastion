using System.Text.Json;

namespace Bastion.Core;

/// <summary>
/// Klientlokal inställning — VILKEN mapp den här installationen synkar mot.
/// Medvetet INTE en del av det delade SyncState/protokollet (samma design
/// som LinuxApp/src/sync.rs::SyncConfig): varje enhet kan peka mot en
/// annan lokal synk-mapp, det är inte data att slå ihop mellan enheter.
/// </summary>
public sealed class SyncConfig
{
    public string? FolderPath { get; set; }

    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".bastion", "sync-config.json");

    public static SyncConfig Load(string path)
    {
        if (!File.Exists(path)) return new SyncConfig();
        try
        {
            return JsonSerializer.Deserialize<SyncConfig>(File.ReadAllText(path)) ?? new SyncConfig();
        }
        catch (JsonException)
        {
            return new SyncConfig();
        }
    }

    public void Save(string path)
    {
        var dir = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.WriteAllText(path, JsonSerializer.Serialize(this));
    }
}
