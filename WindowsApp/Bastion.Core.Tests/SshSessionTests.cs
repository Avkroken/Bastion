using Bastion.Core;

namespace Bastion.Core.Tests;

/// <summary>
/// Riktiga integrationstester mot localhosts sshd — kräver
/// BASTION_TEST_SSH_KEY (en nyckel redan tillagd i ~/.ssh/authorized_keys,
/// sätts upp/rivs manuellt av testskriptet, inte av testerna själva). Körs
/// inte i vanlig `dotnet test` (ingen nyckel satt i CI), samma mönster som
/// LinuxApp/src/ssh.rs #[ignore]-testerna.
/// </summary>
public class SshSessionTests
{
    private static bool HasTestKey => Environment.GetEnvironmentVariable("BASTION_TEST_SSH_KEY") is not null;

    private static Host BuildTestHost()
    {
        var keyPath = Environment.GetEnvironmentVariable("BASTION_TEST_SSH_KEY")!;
        var user = Environment.GetEnvironmentVariable("USER") ?? Environment.UserName;
        var host = Host.Create("test", "127.0.0.1", user);
        host.Auth = new HostAuth.KeyFile(keyPath);
        return host;
    }

    [Fact]
    public void ConnectsToRealLocalhostSshdAndGetsAShellPrompt()
    {
        if (!HasTestKey) return; // se klassdoc
        var host = BuildTestHost();
        var knownHosts = new KnownHosts(null); // i-minne, rör inte den riktiga filen i detta test
        using var session = SshSession.Connect(host, null, knownHosts);

        var deadline = DateTime.UtcNow.AddSeconds(10);
        var gotData = false;
        while (DateTime.UtcNow < deadline)
        {
            if (session.Shell.DataAvailable) { gotData = true; break; }
            Thread.Sleep(50);
        }
        Assert.True(gotData, "fick aldrig någon data tillbaka från fjärrskalet");
    }

    [Fact]
    public void TypingExitInTheShellClosesTheSession()
    {
        if (!HasTestKey) return;
        var host = BuildTestHost();
        var knownHosts = new KnownHosts(null);
        using var session = SshSession.Connect(host, null, knownHosts);

        // Vänta in första skalpromptens data innan vi skriver något.
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (DateTime.UtcNow < deadline && !session.Shell.DataAvailable) Thread.Sleep(50);
        Assert.True(session.Shell.DataAvailable, "fick aldrig en initial prompt från skalet");
        _ = session.Shell.Read();

        var closed = false;
        session.Shell.Closed += (_, _) => closed = true;
        session.Shell.WriteLine("exit");

        deadline = DateTime.UtcNow.AddSeconds(10);
        while (DateTime.UtcNow < deadline && !closed) Thread.Sleep(100);
        Assert.True(closed, "sessionen stängdes aldrig efter att exit skrevs i skalet (ShellStream.Closed-eventet fyrade inte)");
    }

    [Fact]
    public void RejectsConnectionWhenHostKeyHasChanged()
    {
        if (!HasTestKey) return;
        var host = BuildTestHost();
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-cs-known-hosts-{Guid.NewGuid()}");
        Directory.CreateDirectory(dir);
        var knownHostsPath = Path.Combine(dir, "known_hosts");
        File.WriteAllText(knownHostsPath, "127.0.0.1:22 ssh-ed25519 FALSKT-INTE-DEN-RIKTIGA-NYCKELN\n");
        var knownHosts = new KnownHosts(knownHostsPath);

        var ex = Assert.ThrowsAny<Exception>(() =>
        {
            using var session = SshSession.Connect(host, null, knownHosts);
        });
        Assert.Contains("HAR ÄNDRATS", ex.ToString());
        Directory.Delete(dir, true);
    }

    [Fact]
    public void RunCommandExecutesARealReadonlyCommandOverSsh()
    {
        if (!HasTestKey) return;
        var host = BuildTestHost();
        var knownHosts = new KnownHosts(null);
        var output = SshSession.RunCommand(host, null, knownHosts, "echo bastion-run-command-ok");
        Assert.Equal("bastion-run-command-ok", output.Trim());
    }
}

/// <summary>
/// Samma tester som ovan, men mot en FRISTÅENDE test-sshd (<see cref="TestSshd"/>)
/// istället för den nyckel-gatade riktiga systemtjänsten — körbara här och i CI
/// utan `BASTION_TEST_SSH_KEY`.
/// </summary>
public class SshSessionStandaloneTests
{
    private static Host BuildHost(TestSshd sshd)
    {
        var host = Host.Create("test", "127.0.0.1", Environment.GetEnvironmentVariable("USER") ?? Environment.UserName);
        host.Port = sshd.Port;
        host.Auth = new HostAuth.KeyFile(sshd.ClientKeyPath);
        return host;
    }

    [Fact]
    public void ConnectsToAStandaloneTestSshdAndGetsAShellPrompt()
    {
        using var sshd = TestSshd.Start();
        if (sshd is null) return;
        var host = BuildHost(sshd);
        var knownHosts = new KnownHosts(null);
        using var session = SshSession.Connect(host, null, knownHosts);

        var deadline = DateTime.UtcNow.AddSeconds(10);
        var gotData = false;
        while (DateTime.UtcNow < deadline)
        {
            if (session.Shell.DataAvailable) { gotData = true; break; }
            Thread.Sleep(50);
        }
        Assert.True(gotData, "fick aldrig någon data tillbaka från fjärrskalet");
    }

    [Fact]
    public void TypingExitInTheShellClosesTheSession()
    {
        using var sshd = TestSshd.Start();
        if (sshd is null) return;
        var host = BuildHost(sshd);
        var knownHosts = new KnownHosts(null);
        using var session = SshSession.Connect(host, null, knownHosts);

        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (DateTime.UtcNow < deadline && !session.Shell.DataAvailable) Thread.Sleep(50);
        Assert.True(session.Shell.DataAvailable, "fick aldrig en initial prompt från skalet");
        _ = session.Shell.Read();

        var closed = false;
        session.Shell.Closed += (_, _) => closed = true;
        session.Shell.WriteLine("exit");

        deadline = DateTime.UtcNow.AddSeconds(10);
        while (DateTime.UtcNow < deadline && !closed) Thread.Sleep(100);
        Assert.True(closed, "sessionen stängdes aldrig efter att exit skrevs i skalet (ShellStream.Closed-eventet fyrade inte)");
    }

    [Fact]
    public void RejectsConnectionWhenHostKeyHasChanged()
    {
        using var sshd = TestSshd.Start();
        if (sshd is null) return;
        var host = BuildHost(sshd);
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-cs-known-hosts-{Guid.NewGuid()}");
        Directory.CreateDirectory(dir);
        var knownHostsPath = Path.Combine(dir, "known_hosts");
        File.WriteAllText(knownHostsPath, $"127.0.0.1:{sshd.Port} ssh-ed25519 FALSKT-INTE-DEN-RIKTIGA-NYCKELN\n");
        var knownHosts = new KnownHosts(knownHostsPath);

        var ex = Assert.ThrowsAny<Exception>(() =>
        {
            using var session = SshSession.Connect(host, null, knownHosts);
        });
        Assert.Contains("HAR ÄNDRATS", ex.ToString());
        Directory.Delete(dir, true);
    }

    [Fact]
    public void RunCommandExecutesARealCommandOverSsh()
    {
        using var sshd = TestSshd.Start();
        if (sshd is null) return;
        var host = BuildHost(sshd);
        var knownHosts = new KnownHosts(null);
        var output = SshSession.RunCommand(host, null, knownHosts, "echo bastion-run-command-ok");
        Assert.Equal("bastion-run-command-ok", output.Trim());
    }
}
