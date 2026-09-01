using System.Net.Sockets;
using Renci.SshNet;
using Renci.SshNet.Common;

namespace Bastion.Core;

/// <summary>
/// SSH-session via SSH.NET, port av samma design som LinuxApp/src/ssh.rs:
/// TOFU-verifiering via <see cref="KnownHosts"/>, en interaktiv shell
/// (<see cref="ShellStream"/>), samt en engångskörning för Docker-liknande
/// kommandon.
///
/// KÄND BEGRÄNSNING (dokumenterad, inte dold): bara Ed25519-identiteter
/// stöds för ssh-agent-autentisering (HostAuth.AgentDefault) — se
/// SshAgent.cs klassdoc. Nyckelfilsautentisering (utan lösenfras) och
/// lösenordsautentisering stöds oförändrat.
/// </summary>
public sealed class SshHostKeyChangedException(string message) : Exception(message);

public sealed class SshSession : IDisposable
{
    private static readonly TimeSpan DefaultKeepAliveInterval = TimeSpan.FromSeconds(30);
    private static readonly TimeSpan DefaultProbeTimeout = TimeSpan.FromSeconds(10);

    private sealed class KeepAliveMonitor
    {
        public CancellationTokenSource Cancellation { get; } = new();
    }

    private readonly SshClient _client;
    private readonly object _keepAliveGate = new();
    private readonly object _probeGate = new();
    private KeepAliveMonitor? _keepAliveMonitor;
    private int _disposed;

    /// <summary>
    /// Processvid signal för interaktiva sessioner vars svarsbärande
    /// liveness-sonder har nått feltröskeln. Signalen innehåller den exakta
    /// sessionen så UI:t kan hitta rätt flik utan att SSH-kärnan känner till
    /// WinUI. Vanlig remote shell-close fortsätter via <see cref="ShellStream.Closed"/>.
    /// </summary>
    public static event Action<SshSession>? SessionConnectionLost;

    public ShellStream Shell { get; }

    private SshSession(SshClient client, ShellStream shell)
    {
        _client = client;
        Shell = shell;
    }

    public static SshSession Connect(Host host, string? password, KnownHosts knownHosts, uint cols = 80, uint rows = 24)
    {
        using var agent = ConnectAgentIfNeeded(host);
        var auth = BuildAuthenticationMethod(host, password, agent);
        var connectionInfo = new ConnectionInfo(host.HostName, (int)host.Port, host.User, auth);
        var client = new SshClient(connectionInfo);
        client.HostKeyReceived += MakeHostKeyHandler(host, knownHosts, throwOnChange: true);

        // `client` (och dess underliggande socket) läcker annars om
        // `Connect()`/`CreateShellStream()` kastar — inklusive det
        // AVSIKTLIGA host-key-ändrat-avslaget ovan, som gör exakt det via
        // `HostKeyReceived`. `RunCommand` nedan gör redan rätt med `using`;
        // detta var en inkonsekvens, inte ett medvetet undantag
        // (CodeRabbit-fynd).
        try
        {
            client.Connect();

            var shell = client.CreateShellStream("xterm-256color", cols, rows, 0, 0, 4096);
            if (!string.IsNullOrEmpty(host.StartupCommand))
            {
                shell.WriteLine(host.StartupCommand);
            }

            var session = new SshSession(client, shell);
            session.StartKeepAlive();
            return session;
        }
        catch
        {
            client.Dispose();
            throw;
        }
    }

    /// <summary>
    /// Startar död-detektering för den befintliga SSH-sessionen. SSH.NETs
    /// inbyggda <c>KeepAliveInterval</c> skickar bara SSH_MSG_IGNORE och kan
    /// därför hålla NAT/brandväggar varma men inte bevisa att motparten svarar.
    /// Sonden använder i stället SSH.NETs kanal-request
    /// <c>keepalive@openssh.com</c>, som kräver success/failure-svar men inte
    /// startar något fjärrkommando. Endast timeout/transportfel räknas som en
    /// missad sond. Efter <paramref name="maxMissed"/> missar stängs sessionen
    /// och <see cref="SessionConnectionLost"/> signalerar UI:t. Normal remote
    /// shell-close fortsätter signaleras av <see cref="ShellStream.Closed"/>.
    /// </summary>
    public void StartKeepAlive(
        TimeSpan? interval = null,
        TimeSpan? probeTimeout = null,
        int maxMissed = 3)
    {
        var effectiveInterval = interval ?? DefaultKeepAliveInterval;
        var effectiveTimeout = probeTimeout ?? DefaultProbeTimeout;
        if (effectiveInterval <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(interval));
        if (effectiveTimeout <= TimeSpan.Zero) throw new ArgumentOutOfRangeException(nameof(probeTimeout));
        if (maxMissed <= 0) throw new ArgumentOutOfRangeException(nameof(maxMissed));

        // Validera SSH.NET-kontraktet synkront. En framtida biblioteksversion
        // som flyttar den interna kanalmetoden ska ge ett tydligt fel här, inte
        // en bakgrundstask som tyst slutar bevaka sessionen.
        SshNetChannelLivenessProbe.ValidateContract();

        var monitor = new KeepAliveMonitor();
        KeepAliveMonitor? previous;
        lock (_keepAliveGate)
        {
            ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
            previous = _keepAliveMonitor;
            _keepAliveMonitor = monitor;
            _ = MonitorLivenessAsync(monitor, effectiveInterval, effectiveTimeout, maxMissed);
        }
        previous?.Cancellation.Cancel();
    }

    public void StopKeepAlive()
    {
        KeepAliveMonitor? monitor;
        lock (_keepAliveGate)
        {
            monitor = _keepAliveMonitor;
            _keepAliveMonitor = null;
        }
        monitor?.Cancellation.Cancel();
    }

    private async Task MonitorLivenessAsync(
        KeepAliveMonitor monitor,
        TimeSpan interval,
        TimeSpan probeTimeout,
        int maxMissed)
    {
        var cancellation = monitor.Cancellation;
        var missed = 0;
        try
        {
            while (!cancellation.IsCancellationRequested)
            {
                await Task.Delay(interval, cancellation.Token).ConfigureAwait(false);

                bool alive;
                lock (_probeGate)
                {
                    if (cancellation.IsCancellationRequested) return;
                    alive = ProbeLiveness(probeTimeout);
                }
                if (cancellation.IsCancellationRequested) return;

                if (alive)
                {
                    missed = 0;
                    continue;
                }

                missed += 1;
                if (missed < maxMissed) continue;

                // Claim ownership under samma lås som Stop/Start använder.
                // Om keepalive hann stoppas eller ersättas får en gammal monitor
                // aldrig stänga den nuvarande sessionen efteråt.
                lock (_keepAliveGate)
                {
                    if (!ReferenceEquals(_keepAliveMonitor, monitor) || cancellation.IsCancellationRequested)
                    {
                        return;
                    }
                    _keepAliveMonitor = null;
                }

                Dispose();
                SessionConnectionLost?.Invoke(this);
                return;
            }
        }
        catch (OperationCanceledException) when (cancellation.IsCancellationRequested)
        {
            // normal StopKeepAlive/Dispose
        }
        finally
        {
            lock (_keepAliveGate)
            {
                if (ReferenceEquals(_keepAliveMonitor, monitor))
                {
                    _keepAliveMonitor = null;
                }
            }
            cancellation.Dispose();
        }
    }

    private bool ProbeLiveness(TimeSpan timeout)
    {
        try
        {
            if (!_client.IsConnected) return false;
        }
        catch (ObjectDisposedException)
        {
            return false;
        }

        var connectionInfo = _client.ConnectionInfo;
        var previousTimeout = connectionInfo.Timeout;
        try
        {
            // ChannelSession.WaitOnHandle använder ConnectionInfo.Timeout.
            // WindowsApp använder denna SshClient endast för den interaktiva
            // shell-kanalen; engångskommandon/SFTP har egna klienter.
            connectionInfo.Timeout = timeout;
            SshNetChannelLivenessProbe.SendAndWaitForReply(Shell);
            return true; // success ELLER failure är ett svar och bevisar liv.
        }
        catch (SshConnectionException)
        {
            return false;
        }
        catch (SshOperationTimeoutException)
        {
            return false;
        }
        catch (SocketException)
        {
            return false;
        }
        catch (ObjectDisposedException)
        {
            return false;
        }
        finally
        {
            connectionInfo.Timeout = previousTimeout;
        }
    }

    /// <summary>
    /// Ansluter till den lokala ssh-agenten OM värden faktiskt använder
    /// `AgentDefault` — annars `null`, ingen anledning att öppna en
    /// agentanslutning i onödan. Anroparen (`Connect`/`RunCommand`) håller
    /// den vid liv genom `using` bara under den synkrona `Connect()`/
    /// `Execute()`-anropet, där SSH.NET faktiskt signerar via den.
    /// </summary>
    internal static SshAgentClient? ConnectAgentIfNeeded(Host host) =>
        host.Auth is HostAuth.AgentDefault ? SshAgentClient.Connect() : null;

    /// <summary>
    /// Delas med <see cref="SftpBrowserSession"/> — samma auth-uppslagning
    /// för SFTP-anslutningar. `agent`: se <see cref="ConnectAgentIfNeeded"/>
    /// — `null` OK för alla auth-typer utom `AgentDefault`.
    /// </summary>
    internal static AuthenticationMethod BuildAuthenticationMethod(Host host, string? password, SshAgentClient? agent = null) => host.Auth switch
    {
        HostAuth.KeyFile kf => new PrivateKeyAuthenticationMethod(host.User, new PrivateKeyFile(kf.Path)),
        HostAuth.AskPassword => new PasswordAuthenticationMethod(host.User, password
            ?? throw new InvalidOperationException("lösenord krävs men saknades")),
        HostAuth.AgentDefault => BuildAgentAuthenticationMethod(host, agent),
        var other => throw new NotSupportedException($"autentiseringstypen {other.GetType().Name} stöds inte i WindowsApp ännu"),
    };

    /// <summary>
    /// En `IPrivateKeySource` PER identitet agenten har laddad —
    /// `PrivateKeyAuthenticationMethod` provar dem i tur och ordning tills
    /// en lyckas (samma "provar ALLA laddade identiteter"-beteende som
    /// redan gäller för `ssh.rs`/`russh`s agent-autentisering).
    /// </summary>
    private static AuthenticationMethod BuildAgentAuthenticationMethod(Host host, SshAgentClient? agent)
    {
        if (agent is null)
        {
            throw new InvalidOperationException("ingen ssh-agent hittades (SSH_AUTH_SOCK/named pipe saknas eller anslutningen misslyckades)");
        }
        var identities = agent.RequestIdentities();
        if (identities.Count == 0)
        {
            throw new InvalidOperationException("ssh-agent har inga (Ed25519-)identiteter laddade");
        }
        var sources = identities.Select(i => (IPrivateKeySource)new AgentPrivateKeySource(agent, i.PublicKeyBlob)).ToArray();
        return new PrivateKeyAuthenticationMethod(host.User, sources);
    }

    /// <summary>
    /// Delad TOFU-verifiering (samma <see cref="KnownHosts"/>-fil) för SshClient/SftpClient
    /// — båda ärver Renci.SshNet.BaseClient och exponerar samma HostKeyReceived-event.
    /// <paramref name="throwOnChange"/> avgör om en ändrad värdnyckel avbryter anslutningen
    /// direkt (interaktiva sessioner) eller bara nekar tillit (engångskommandon/SFTP, där
    /// SSH.NET själv kastar ett anslutningsfel om <c>CanTrust</c> är false).
    /// </summary>
    internal static EventHandler<HostKeyEventArgs> MakeHostKeyHandler(Host host, KnownHosts knownHosts, bool throwOnChange) =>
        (_, e) =>
        {
            var keyString = $"{e.HostKeyName} {Convert.ToBase64String(e.HostKey)}";
            var result = knownHosts.Check(host.HostName, (int)host.Port, keyString);
            e.CanTrust = result.Verdict != KnownHostVerdict.Changed;
            if (throwOnChange && result.Verdict == KnownHostVerdict.Changed)
            {
                throw new SshHostKeyChangedException(
                    $"VÄRDNYCKELN FÖR {host.HostName}:{host.Port} HAR ÄNDRATS — möjlig man-i-mitten-attack " +
                    $"eller en ombyggd server. Lagrad: \"{result.StoredKey}\" Ny: \"{keyString}\". Om ändringen " +
                    "är väntad, ta bort motsvarande rad i ~/.bastion/known_hosts manuellt.");
            }
        };

    /// <summary>Kör ETT kommando över en fristående anslutning (motsvarar ssh::run_command).</summary>
    public static string RunCommand(Host host, string? password, KnownHosts knownHosts, string command)
    {
        using var agent = ConnectAgentIfNeeded(host);
        var auth = BuildAuthenticationMethod(host, password, agent);
        var connectionInfo = new ConnectionInfo(host.HostName, (int)host.Port, host.User, auth);
        using var client = new SshClient(connectionInfo);
        client.HostKeyReceived += MakeHostKeyHandler(host, knownHosts, throwOnChange: false);
        client.Connect();
        using var cmd = client.CreateCommand(command);
        // Utan detta kan en hängande/ovanligt långsam fjärrprocess (samma
        // användningsfall som ssh.rs::COMMAND_TIMEOUT — Docker-listor/
        // -loggar) blockera anropet obestämt (CodeRabbit-fynd).
        cmd.CommandTimeout = TimeSpan.FromSeconds(30);
        return cmd.Execute();
    }

    public void Dispose()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0) return;

        StopKeepAlive();
        try
        {
            Shell.Dispose();
        }
        catch (ObjectDisposedException)
        {
            // Sessionen kan redan ha städats via fjärrstängning.
        }
        catch (SshConnectionException)
        {
            // En död transport får inte stoppa lokal städning.
        }
        catch (SocketException)
        {
            // Samma transportfall på socketnivå.
        }

        try
        {
            if (_client.IsConnected) _client.Disconnect();
        }
        catch (ObjectDisposedException)
        {
            // Redan städad av en parallell fjärrstängning.
        }
        catch (SshConnectionException)
        {
            // Transporten är redan bruten; Dispose nedan släpper resurserna.
        }
        catch (SocketException)
        {
            // Transporten är redan bruten på socketnivå.
        }
        _client.Dispose();
    }
}
