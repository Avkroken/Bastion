using System.Text.Json;

namespace Bastion.Core;

/// <summary>Port av protocol SyncProvider — se SYNC_PROTOCOL.md.</summary>
public interface ISyncProvider
{
    SyncState? Pull();
    void Push(SyncState state);
}

/// <summary>
/// Port av FolderSyncProvider — en JSON-fil i en mapp som något annat synkar
/// mellan enheter (Syncthing, en klonad Git-mapp, en krypterad disk).
/// </summary>
public sealed class FolderSyncProvider : ISyncProvider
{
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };
    private readonly string _path;

    public FolderSyncProvider(string path) => _path = path;

    public SyncState? Pull()
    {
        if (!File.Exists(_path)) return null;
        return JsonSerializer.Deserialize<SyncState>(File.ReadAllText(_path), JsonOptions);
    }

    public void Push(SyncState state)
    {
        var dir = Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.WriteAllText(_path, JsonSerializer.Serialize(state, JsonOptions));
    }
}
