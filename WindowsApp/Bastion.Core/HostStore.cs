using System.Text.Json;

namespace Bastion.Core;

/// <summary>
/// Port av HostStore.swift — persistent host-databas, `~/.bastion/hosts.json`.
/// Samma fil kan läsas/skrivas av App/, Android, LinuxApp och WindowsApp.
/// </summary>
public sealed class HostStore
{
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    private readonly string _path;
    private SyncState _state;
    private readonly object _lock = new();

    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".bastion", "hosts.json");

    public HostStore(string path)
    {
        _path = path;
        _state = Load(path);
    }

    private static SyncState Load(string path)
    {
        if (!File.Exists(path)) return new SyncState();
        var text = File.ReadAllText(path);
        try
        {
            return JsonSerializer.Deserialize<SyncState>(text, JsonOptions) ?? new SyncState();
        }
        catch (JsonException)
        {
            // Äldre format: en ren Host[]-array utan SyncState-omslag.
            try
            {
                var hosts = JsonSerializer.Deserialize<List<Host>>(text, JsonOptions) ?? new();
                return new SyncState { Hosts = hosts };
            }
            catch (JsonException)
            {
                return new SyncState();
            }
        }
    }

    public List<Host> All()
    {
        lock (_lock)
            return _state.Hosts.OrderBy(h => h.Alias, StringComparer.OrdinalIgnoreCase).ToList();
    }

    public void Upsert(Host host)
    {
        lock (_lock)
        {
            host.ModifiedAt = ReferenceDate.Now();
            _state.Tombstones.Remove(host.Id);
            var index = _state.Hosts.FindIndex(h => h.Id == host.Id);
            if (index >= 0) _state.Hosts[index] = host;
            else _state.Hosts.Add(host);
            Persist();
        }
    }

    public void Delete(Guid id)
    {
        lock (_lock)
        {
            _state.Hosts.RemoveAll(h => h.Id == id);
            _state.Tombstones[id] = ReferenceDate.Now();
            Persist();
        }
    }

    public void Sync(ISyncProvider provider)
    {
        lock (_lock)
        {
            var remote = provider.Pull() ?? new SyncState();
            _state = SyncEngine.Merge(_state, remote);
            Persist();
            provider.Push(_state);
        }
    }

    private void Persist()
    {
        var dir = Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.WriteAllText(_path, JsonSerializer.Serialize(_state, JsonOptions));
    }
}
