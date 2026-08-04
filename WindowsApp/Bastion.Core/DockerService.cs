using System.Text.RegularExpressions;

namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/DockerService.swift (samma design som
/// LinuxApp/src/docker.rs). Kommandobyggarna och parsningen är rena
/// funktioner; SSH-lagret (<see cref="SshSession.RunCommand"/>) är tunt
/// ovanpå. Containerreferenser VALIDERAS innan de sätts in i ett
/// shell-kommando — annars vore "name; rm -rf /" en injektion.
/// </summary>
public sealed record DockerContainer(string Id, string Name, string Image, string Status)
{
    /// <summary>Härleds ur statustexten ("Up 3 days" = igång, "Exited (0)…" = stoppad).</summary>
    public bool IsRunning => Status.StartsWith("Up", StringComparison.Ordinal);
}

public sealed class DockerInvalidReferenceException(string reference)
    : Exception($"ogiltig container-referens: \"{reference}\"")
{
    public string Reference { get; } = reference;
}

public static partial class DockerService
{
    [GeneratedRegex("^[A-Za-z0-9][A-Za-z0-9_.-]*$")]
    private static partial Regex ReferencePattern();

    private const string ListFormat = "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}";

    /// <summary>Docker-namn: [a-zA-Z0-9][a-zA-Z0-9_.-]*, max 128 tecken. Allt annat avvisas.</summary>
    public static string Validate(string reference)
    {
        if (reference.Length <= 128 && ReferencePattern().IsMatch(reference)) return reference;
        throw new DockerInvalidReferenceException(reference);
    }

    public static string ListCommand(bool all) =>
        $"docker ps{(all ? " -a" : "")} --format '{ListFormat}' 2>/dev/null";

    public static string StartCommand(string reference) => $"docker start {Validate(reference)}";

    public static string StopCommand(string reference) => $"docker stop {Validate(reference)}";

    public static string RestartCommand(string reference) => $"docker restart {Validate(reference)}";

    public static string LogsCommand(string reference, int tail = 200)
    {
        var n = Math.Max(1, tail);
        return $"docker logs --tail {n} {Validate(reference)} 2>&1";
    }

    /// <summary>Interaktiv shell i en container. Faller tillbaka till sh om bash saknas.</summary>
    public static string ExecShellCommand(string reference)
    {
        var r = Validate(reference);
        return $"docker exec -it {r} sh -c 'command -v bash >/dev/null && exec bash || exec sh'";
    }

    public static IReadOnlyList<DockerContainer> ParseList(string output)
    {
        var list = new List<DockerContainer>();
        foreach (var line in output.Split(['\n', '\r'], StringSplitOptions.RemoveEmptyEntries))
        {
            var f = line.Split('|');
            if (f.Length < 4 || f[0].Length == 0) continue;
            list.Add(new DockerContainer(f[0], f[1], f[2], f[3]));
        }
        return list;
    }

    public static IReadOnlyList<DockerContainer> List(Host host, string? password, KnownHosts knownHosts, bool all = true) =>
        ParseList(SshSession.RunCommand(host, password, knownHosts, ListCommand(all)));

    public static void Start(Host host, string? password, KnownHosts knownHosts, string reference) =>
        SshSession.RunCommand(host, password, knownHosts, StartCommand(reference));

    public static void Stop(Host host, string? password, KnownHosts knownHosts, string reference) =>
        SshSession.RunCommand(host, password, knownHosts, StopCommand(reference));

    public static void Restart(Host host, string? password, KnownHosts knownHosts, string reference) =>
        SshSession.RunCommand(host, password, knownHosts, RestartCommand(reference));

    public static string Logs(Host host, string? password, KnownHosts knownHosts, string reference, int tail = 200) =>
        SshSession.RunCommand(host, password, knownHosts, LogsCommand(reference, tail));
}
