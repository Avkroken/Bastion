namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/KnownHosts.swift (samma fil/format som
/// LinuxApp/src/known_hosts.rs): trust-on-first-use-lagring av
/// värdnycklar, `~/.bastion/known_hosts`, en rad per värd
/// (`host:port ssh-ed25519 AAAA...`).
/// </summary>
public enum KnownHostVerdict { Trusted, Learned, Changed }

public sealed record KnownHostResult(KnownHostVerdict Verdict, string? StoredKey = null);

public sealed class KnownHosts
{
    private readonly string? _path;
    private readonly Dictionary<string, string> _entries;
    private readonly object _lock = new();

    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".bastion", "known_hosts");

    public KnownHosts(string? path)
    {
        _path = path;
        _entries = Load(path);
    }

    private static Dictionary<string, string> Load(string? path)
    {
        var result = new Dictionary<string, string>();
        if (path is null || !File.Exists(path)) return result;
        foreach (var line in File.ReadAllLines(path))
        {
            var spaceIndex = line.IndexOf(' ');
            if (spaceIndex <= 0) continue;
            result[line[..spaceIndex]] = line[(spaceIndex + 1)..];
        }
        return result;
    }

    public KnownHostResult Check(string host, int port, string keyString)
    {
        lock (_lock)
        {
            var id = $"{host}:{port}";
            if (_entries.TryGetValue(id, out var stored))
                return stored == keyString
                    ? new KnownHostResult(KnownHostVerdict.Trusted)
                    : new KnownHostResult(KnownHostVerdict.Changed, stored);

            _entries[id] = keyString;
            Append(id, keyString);
            return new KnownHostResult(KnownHostVerdict.Learned);
        }
    }

    private void Append(string id, string keyString)
    {
        if (_path is null) return;
        var dir = Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.AppendAllText(_path, $"{id} {keyString}\n");
    }
}
