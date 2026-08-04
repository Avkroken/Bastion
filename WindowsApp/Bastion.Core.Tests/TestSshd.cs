using System.Diagnostics;
using System.Net.Sockets;

namespace Bastion.Core.Tests;

/// <summary>
/// Fristående, minimal sshd-instans på en slumpad hög port med en egen
/// konfigfil (läser INTE `/etc/ssh/sshd_config`) — samma teknik som
/// LinuxApp/src/port_forward.rs::TestSshd. Träffas alltså inte av
/// sandlådekontots `DenyUsers`-restriktion i systemtjänstens sshd_config,
/// som bara gäller den RIKTIGA systemtjänsten på port 22 — se
/// SftpBrowserSessionTests klassdoc för varför den befintliga,
/// nyckel-gated integrationstestet aldrig kan köra här.
/// </summary>
public sealed class TestSshd : IDisposable
{
    private readonly Process _process;
    private readonly string _dir;
    public int Port { get; }
    public string ClientKeyPath { get; }

    private TestSshd(Process process, string dir, int port, string clientKeyPath)
    {
        _process = process;
        _dir = dir;
        Port = port;
        ClientKeyPath = clientKeyPath;
    }

    /// <summary>Returnerar null (inte ett kastat undantag) om miljön saknar sshd/ssh-keygen — anroparen hoppar då över testet, samma "hoppa över, inte fail" som Rust-sidans motsvarighet.</summary>
    public static TestSshd? Start()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-cs-sshd-{Guid.NewGuid()}");
        Directory.CreateDirectory(dir);

        var hostKey = Path.Combine(dir, "hostkey");
        if (!RunKeygen(hostKey)) return null;
        var clientKey = Path.Combine(dir, "client_key");
        if (!RunKeygen(clientKey)) return null;

        var authorizedKeys = Path.Combine(dir, "authorized_keys");
        File.Copy(clientKey + ".pub", authorizedKeys);

        int port;
        using (var probe = new TcpListener(System.Net.IPAddress.Loopback, 0))
        {
            probe.Start();
            port = ((System.Net.IPEndPoint)probe.LocalEndpoint).Port;
        }

        var configPath = Path.Combine(dir, "sshd_config");
        File.WriteAllText(configPath,
            $"Port {port}\nListenAddress 127.0.0.1\nHostKey {hostKey}\nAuthorizedKeysFile {authorizedKeys}\n" +
            "PubkeyAuthentication yes\nPasswordAuthentication no\nUsePAM no\nStrictModes no\n" +
            $"Subsystem sftp internal-sftp\nPidFile {Path.Combine(dir, "pid")}\n");

        var psi = new ProcessStartInfo("/usr/sbin/sshd")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        psi.ArgumentList.Add("-f");
        psi.ArgumentList.Add(configPath);
        psi.ArgumentList.Add("-D");
        psi.ArgumentList.Add("-e");
        var process = Process.Start(psi);
        if (process is null)
        {
            Directory.Delete(dir, recursive: true);
            return null;
        }

        for (var i = 0; i < 50; i++)
        {
            try
            {
                using var probeClient = new TcpClient();
                probeClient.Connect("127.0.0.1", port);
                return new TestSshd(process, dir, port, clientKey);
            }
            catch (SocketException)
            {
                Thread.Sleep(100);
            }
        }

        process.Kill();
        Directory.Delete(dir, recursive: true);
        return null;
    }

    private static bool RunKeygen(string path)
    {
        var psi = new ProcessStartInfo("ssh-keygen")
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        psi.ArgumentList.Add("-q");
        psi.ArgumentList.Add("-N");
        psi.ArgumentList.Add("");
        psi.ArgumentList.Add("-t");
        psi.ArgumentList.Add("ed25519");
        psi.ArgumentList.Add("-f");
        psi.ArgumentList.Add(path);
        using var process = Process.Start(psi);
        if (process is null) return false;
        process.WaitForExit();
        return process.ExitCode == 0;
    }

    public void Dispose()
    {
        try
        {
            _process.Kill();
            _process.WaitForExit(2000);
        }
        catch
        {
            // best effort — processen kan redan ha dött
        }
        try
        {
            Directory.Delete(_dir, recursive: true);
        }
        catch
        {
            // best effort
        }
    }
}
