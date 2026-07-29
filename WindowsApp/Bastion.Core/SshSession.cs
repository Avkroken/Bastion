using Renci.SshNet;
using Renci.SshNet.Common;

namespace Bastion.Core;

/// <summary>
/// SSH-session via SSH.NET, port av samma design som LinuxApp/src/ssh.rs:
/// TOFU-verifiering via <see cref="KnownHosts"/>, en interaktiv shell
/// (<see cref="ShellStream"/>), samt en engångskörning för Docker-liknande
/// kommandon.
///
/// KÄND BEGRÄNSNING (dokumenterad, inte dold, samma som ssh.rs): bara
/// nyckelfilsautentisering (utan lösenfras) och lösenordsautentisering
/// stöds. ssh-agent-baserad autentisering (HostAuth.AgentDefault) är INTE
/// porterad — SSH.NET har inget inbyggt agent-protokollstöd.
/// </summary>
public sealed class SshHostKeyChangedException(string message) : Exception(message);

public sealed class SshSession : IDisposable
{
    private readonly SshClient _client;
    public ShellStream Shell { get; }

    private SshSession(SshClient client, ShellStream shell)
    {
        _client = client;
        Shell = shell;
    }

    public static SshSession Connect(Host host, string? password, KnownHosts knownHosts, uint cols = 80, uint rows = 24)
    {
        var auth = BuildAuthenticationMethod(host, password);
        var connectionInfo = new ConnectionInfo(host.HostName, (int)host.Port, host.User, auth);
        var client = new SshClient(connectionInfo);

        client.HostKeyReceived += (_, e) =>
        {
            var keyString = $"{e.HostKeyName} {Convert.ToBase64String(e.HostKey)}";
            var result = knownHosts.Check(host.HostName, (int)host.Port, keyString);
            e.CanTrust = result.Verdict != KnownHostVerdict.Changed;
            if (result.Verdict == KnownHostVerdict.Changed)
            {
                throw new SshHostKeyChangedException(
                    $"VÄRDNYCKELN FÖR {host.HostName}:{host.Port} HAR ÄNDRATS — möjlig man-i-mitten-attack " +
                    $"eller en ombyggd server. Lagrad: \"{result.StoredKey}\" Ny: \"{keyString}\". Om ändringen " +
                    "är väntad, ta bort motsvarande rad i ~/.bastion/known_hosts manuellt.");
            }
        };

        client.Connect();

        var shell = client.CreateShellStream("xterm-256color", cols, rows, 0, 0, 4096);
        if (!string.IsNullOrEmpty(host.StartupCommand))
        {
            shell.WriteLine(host.StartupCommand);
        }

        return new SshSession(client, shell);
    }

    private static AuthenticationMethod BuildAuthenticationMethod(Host host, string? password) => host.Auth switch
    {
        HostAuth.KeyFile kf => new PrivateKeyAuthenticationMethod(host.User, new PrivateKeyFile(kf.Path)),
        HostAuth.AskPassword => new PasswordAuthenticationMethod(host.User, password
            ?? throw new InvalidOperationException("lösenord krävs men saknades")),
        var other => throw new NotSupportedException($"autentiseringstypen {other.GetType().Name} stöds inte i WindowsApp ännu"),
    };

    /// <summary>Kör ETT kommando över en fristående anslutning (motsvarar ssh::run_command).</summary>
    public static string RunCommand(Host host, string? password, KnownHosts knownHosts, string command)
    {
        var auth = BuildAuthenticationMethod(host, password);
        var connectionInfo = new ConnectionInfo(host.HostName, (int)host.Port, host.User, auth);
        using var client = new SshClient(connectionInfo);
        client.HostKeyReceived += (_, e) =>
        {
            var keyString = $"{e.HostKeyName} {Convert.ToBase64String(e.HostKey)}";
            var result = knownHosts.Check(host.HostName, (int)host.Port, keyString);
            e.CanTrust = result.Verdict != KnownHostVerdict.Changed;
        };
        client.Connect();
        using var cmd = client.CreateCommand(command);
        return cmd.Execute();
    }

    public void Dispose()
    {
        Shell.Dispose();
        _client.Disconnect();
        _client.Dispose();
    }
}
