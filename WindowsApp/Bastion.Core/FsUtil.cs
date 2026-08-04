namespace Bastion.Core;

/// <summary>
/// Delad hjälpare för atomär filpersistens. Konsoliderat hit efter en
/// CodeRabbit-granskning (PR #216) som pekade på samma icke-atomära
/// <c>File.WriteAllText</c>/<c>File.WriteAllBytes</c>-mönster på flera
/// ställen (HostStore, SyncConfig, SyncCrypto, SyncProvider): ett krasch/
/// strömavbrott mitt i en direkt skrivning lämnar en trunkerad fil, vilket
/// <c>Load</c>-sidan annars inte kunde skilja från "filen finns inte än" —
/// nästa skrivning skulle då permanent skriva över den trunkerade filen
/// med ett tomt tillstånd.
/// </summary>
public static class FsUtil
{
    /// <summary>
    /// Skriver <paramref name="data"/> till <paramref name="path"/> atomärt:
    /// en temporär fil i SAMMA katalog (garanterar att <see cref="File.Move"/>
    /// nedan är en atomär filsystemsoperation, inte en cross-filesystem-
    /// kopiering) skrivs och flushas, byts sedan in över målet.
    /// </summary>
    public static void AtomicWrite(string path, byte[] data)
    {
        var dir = Path.GetDirectoryName(path);
        if (string.IsNullOrEmpty(dir))
        {
            throw new ArgumentException("sökvägen saknar en förälderkatalog", nameof(path));
        }
        var tmpPath = Path.Combine(dir, $".{Path.GetFileName(path)}.tmp.{Guid.NewGuid():N}");
        try
        {
            using (var stream = new FileStream(tmpPath, FileMode.CreateNew, FileAccess.Write))
            {
                stream.Write(data, 0, data.Length);
                stream.Flush(flushToDisk: true);
            }
            File.Move(tmpPath, path, overwrite: true);
        }
        catch
        {
            File.Delete(tmpPath);
            throw;
        }
    }

    public static void AtomicWriteText(string path, string text) =>
        AtomicWrite(path, System.Text.Encoding.UTF8.GetBytes(text));
}
