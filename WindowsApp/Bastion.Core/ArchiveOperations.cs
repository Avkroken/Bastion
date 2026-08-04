namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/ArchiveOperations.swift (samma design som
/// LinuxApp/src/archive.rs). SFTP version 3 har ingen egen arkivsemantik —
/// shellar ut till tar/zip över <see cref="SshSession.RunCommand"/> (samma
/// engångsexec-mönster som Docker-vyn). Sökvägar VALIDERAS inte mot en
/// whitelist (filnamn kan legitimt innehålla mellanslag/unicode) — istället
/// citeras varje sökväg för sig med enkla citattecken (POSIX-shell-säkert).
/// </summary>
public static class ArchiveOperations
{
    /// <summary>Enkla citattecken runt s, med inbäddade ' eskapade som '\'' — standard POSIX-shell-säkert sätt att citera GODTYCKLIG text.</summary>
    public static string ShellQuote(string s) => $"'{s.Replace("'", "'\\''")}'";

    public static string CreateTarGzCommand(IReadOnlyList<string> paths, string archiveName, string directory)
    {
        var quotedPaths = string.Join(" ", paths.Select(ShellQuote));
        return $"cd {ShellQuote(directory)} && tar czf {ShellQuote(archiveName)} -- {quotedPaths}";
    }

    public static string ExtractTarGzCommand(string archiveName, string directory) =>
        $"cd {ShellQuote(directory)} && tar xzf {ShellQuote(archiveName)}";

    /// <summary>
    /// ./-prefix på arkivnamnet + -- före sökvägarna — zip tar arkivnamnet
    /// som ett rent positionellt argument, så ett namn som börjar med -
    /// skulle annars tolkas som en flagga.
    /// </summary>
    public static string CreateZipCommand(IReadOnlyList<string> paths, string archiveName, string directory)
    {
        var quotedPaths = string.Join(" ", paths.Select(ShellQuote));
        var safeArchiveName = $"./{archiveName}";
        return $"cd {ShellQuote(directory)} && zip -r -q {ShellQuote(safeArchiveName)} -- {quotedPaths}";
    }

    public static string ExtractZipCommand(string archiveName, string directory) =>
        $"cd {ShellQuote(directory)} && unzip -o -q {ShellQuote($"./{archiveName}")}";
}
