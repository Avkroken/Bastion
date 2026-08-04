using System.Text;
using Renci.SshNet;
using Renci.SshNet.Sftp;

namespace Bastion.Core;

/// <summary>
/// Port av App/SFTPBrowserModel.swifts kärnfunktioner (samma bläddringsmodell
/// som LinuxApp/src/sftp.rs) — här ovanpå SSH.NETs inbyggda <see cref="SftpClient"/>
/// istället för en egen SFTP-protokollimplementation (Rust/Swift saknar en
/// färdig SSH-klient med SFTP inbyggt; SSH.NET har det redan). En anslutning
/// hålls öppen och återanvänds för hela bläddringen — motsvarar Swiftsidans
/// <c>ensureClient()</c>-cache / Rusts en-bakgrundstråd-per-flik.
/// </summary>
public sealed record SftpEntry(string Name, bool IsDirectory, long Size);

public sealed class SftpBrowserSession : IDisposable
{
    private readonly SftpClient _client;

    private SftpBrowserSession(SftpClient client) => _client = client;

    public static SftpBrowserSession Connect(Host host, string? password, KnownHosts knownHosts)
    {
        using var agent = SshSession.ConnectAgentIfNeeded(host);
        var auth = SshSession.BuildAuthenticationMethod(host, password, agent);
        var connectionInfo = new ConnectionInfo(host.HostName, (int)host.Port, host.User, auth);
        var client = new SftpClient(connectionInfo);
        client.HostKeyReceived += SshSession.MakeHostKeyHandler(host, knownHosts, throwOnChange: true);
        client.Connect();
        return new SftpBrowserSession(client);
    }

    /// <summary>Katalogen listas alltid mapp-först, sedan alfabetiskt inom varje grupp (samma sortering som App/LinuxApp).</summary>
    public IReadOnlyList<SftpEntry> List(string path) =>
        _client.ListDirectory(path)
            .Where(f => f.Name != "." && f.Name != "..")
            .Select(f => new SftpEntry(f.Name, f.IsDirectory, f.Length))
            .OrderByDescending(e => e.IsDirectory)
            .ThenBy(e => e.Name, StringComparer.OrdinalIgnoreCase)
            .ToList();

    public byte[] ReadFile(string path)
    {
        using var stream = new MemoryStream();
        _client.DownloadFile(path, stream);
        return stream.ToArray();
    }

    /// <summary>Skapar filen om den inte finns (SFTP v3 saknar en egen "skapa om saknas"-flagga i SSH.NETs UploadFile, men den öppnar/skapar redan korrekt).</summary>
    public void WriteFile(string path, byte[] data)
    {
        using var stream = new MemoryStream(data);
        _client.UploadFile(stream, path, canOverride: true);
    }

    /// <summary>Försöker avkoda som UTF-8 STRIKT (kastar på ogiltiga byte-sekvenser) — samma säkerhetsmarginal som Swiftsidans <c>String(bytes:encoding:.utf8)</c>: binärt innehåll ska aldrig tyst tolkas som text och riskera att sparas över.</summary>
    public static bool TryDecodeUtf8(byte[] bytes, out string text)
    {
        try
        {
            text = new UTF8Encoding(false, throwOnInvalidBytes: true).GetString(bytes);
            return true;
        }
        catch (DecoderFallbackException)
        {
            text = "";
            return false;
        }
    }

    public void CreateDirectory(string path) => _client.CreateDirectory(path);

    public void RemoveFile(string path) => _client.DeleteFile(path);

    public void RemoveDirectory(string path) => _client.DeleteDirectory(path);

    public void Rename(string from, string to) => _client.RenameFile(from, to);

    /// <summary>mode: oktalt heltal, t.ex. 0b111101101 för "755" — samma notation som chmod.</summary>
    public void SetPermissions(string path, short mode) => _client.ChangePermissions(path, mode);

    public void SetOwner(string path, int uid, int gid)
    {
        var attrs = _client.GetAttributes(path);
        attrs.UserId = uid;
        attrs.GroupId = gid;
        _client.SetAttributes(path, attrs);
    }

    public void Dispose() => _client.Dispose();
}
