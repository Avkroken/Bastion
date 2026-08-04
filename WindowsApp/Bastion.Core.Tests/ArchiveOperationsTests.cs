using Bastion.Core;
using Xunit;

namespace Bastion.Core.Tests;

public class ArchiveOperationsTests
{
    [Fact]
    public void ShellQuoteEscapesEmbeddedSingleQuotes()
    {
        Assert.Equal("'plain'", ArchiveOperations.ShellQuote("plain"));
        Assert.Equal("'with space'", ArchiveOperations.ShellQuote("with space"));
        Assert.Equal(@"'it'\''s'", ArchiveOperations.ShellQuote("it's"));
    }

    /// <summary>Bevisar att en filnamn-injektion FAKTISKT nollställs av citeringen — en RIKTIG shell (/bin/sh -c) tolkar hela kommandot som EN sökväg.</summary>
    [Fact]
    public void ShellQuoteSurvivesRealShellParsing()
    {
        var malicious = $"innocent'; touch /tmp/bastion-cs-injection-proof-{Guid.NewGuid()}; echo '";
        var quoted = ArchiveOperations.ShellQuote(malicious);

        // ArgumentList (inte Arguments-strängen) så /bin/sh -c får EXAKT en sträng, orörd av .NET:s egen citering.
        var psi = new System.Diagnostics.ProcessStartInfo("/bin/sh") { RedirectStandardOutput = true, UseShellExecute = false };
        psi.ArgumentList.Add("-c");
        psi.ArgumentList.Add($"printf '%s' {quoted}");

        using var process = System.Diagnostics.Process.Start(psi)!;
        var output = process.StandardOutput.ReadToEnd();
        process.WaitForExit();

        Assert.Equal(malicious, output);
    }

    [Fact]
    public void CreateTarGzCommandMatchesReferenceImplementation() =>
        Assert.Equal(
            "cd '/home/x' && tar czf 'out.tar.gz' -- 'a.txt' 'b.txt'",
            ArchiveOperations.CreateTarGzCommand(new[] { "a.txt", "b.txt" }, "out.tar.gz", "/home/x"));

    [Fact]
    public void ExtractTarGzCommandMatchesReferenceImplementation() =>
        Assert.Equal("cd '/home/x' && tar xzf 'out.tar.gz'", ArchiveOperations.ExtractTarGzCommand("out.tar.gz", "/home/x"));

    [Fact]
    public void CreateZipCommandMatchesReferenceImplementation() =>
        Assert.Equal(
            "cd '/home/x' && zip -r -q './out.zip' -- 'a.txt'",
            ArchiveOperations.CreateZipCommand(new[] { "a.txt" }, "out.zip", "/home/x"));

    [Fact]
    public void ExtractZipCommandMatchesReferenceImplementation() =>
        Assert.Equal("cd '/home/x' && unzip -o -q './out.zip'", ArchiveOperations.ExtractZipCommand("out.zip", "/home/x"));
}
