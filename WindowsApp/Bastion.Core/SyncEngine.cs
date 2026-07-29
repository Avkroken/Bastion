namespace Bastion.Core;

/// <summary>
/// Port av SyncEngine.merge — deterministisk last-write-wins-sammanslagning,
/// identisk med Swift och LinuxApp/src/sync.rs. Se SYNC_PROTOCOL.md i
/// repo-roten för den formella specifikationen.
/// </summary>
public static class SyncEngine
{
    public static SyncState Merge(SyncState a, SyncState b)
    {
        var newestHost = new Dictionary<Guid, Host>();
        foreach (var h in a.Hosts.Concat(b.Hosts))
        {
            if (!newestHost.TryGetValue(h.Id, out var existing) || h.ModifiedAt.Seconds >= existing.ModifiedAt.Seconds)
                newestHost[h.Id] = h;
        }

        var tomb = new Dictionary<Guid, ReferenceDate>();
        foreach (var (id, t) in a.Tombstones.Concat(b.Tombstones))
        {
            if (!tomb.TryGetValue(id, out var existing) || t.Seconds > existing.Seconds)
                tomb[id] = t;
        }

        var liveHosts = new List<Host>();
        var finalTombstones = new Dictionary<Guid, ReferenceDate>();
        var allIds = newestHost.Keys.Union(tomb.Keys);
        foreach (var id in allIds)
        {
            var hasHost = newestHost.TryGetValue(id, out var host);
            var hasTomb = tomb.TryGetValue(id, out var deletedAt);
            if (hasHost && hasTomb)
            {
                if (deletedAt.Seconds >= host!.ModifiedAt.Seconds) finalTombstones[id] = deletedAt;
                else liveHosts.Add(host);
            }
            else if (hasHost)
            {
                liveHosts.Add(host!);
            }
            else if (hasTomb)
            {
                finalTombstones[id] = deletedAt;
            }
        }

        return new SyncState
        {
            Hosts = liveHosts.OrderBy(h => h.Alias, StringComparer.OrdinalIgnoreCase).ToList(),
            Tombstones = finalTombstones,
        };
    }
}
