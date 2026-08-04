using System.Buffers.Binary;
using System.Diagnostics;
using System.Text;
using Bastion.Core;
using Xunit;

namespace Bastion.Core.Tests;

/// <summary>
/// Riktiga tester mot en genuin ssh-agent-process (startad av testskriptet
/// via `ssh-agent -a &lt;sockel&gt;` + `ssh-add`, INTE mockad) — hoppar tyst
/// över om `SSH_AUTH_SOCK` inte är satt (samma "hoppa över, kräv inte en
/// specifik miljö"-princip som övriga `TestSshd`-baserade tester i den här
/// svit).
/// </summary>
public class SshAgentTests
{
    private static bool HasAgent => !string.IsNullOrEmpty(Environment.GetEnvironmentVariable("SSH_AUTH_SOCK"));

    [Fact]
    public void RequestIdentitiesListsTheLoadedEd25519Key()
    {
        if (!HasAgent) return;
        using var agent = SshAgentClient.Connect();
        Assert.NotNull(agent);

        var identities = agent!.RequestIdentities();
        Assert.NotEmpty(identities);
        Assert.All(identities, i => Assert.Equal("ssh-ed25519", ExtractAlgorithmName(i.PublicKeyBlob)));
    }

    /// <summary>
    /// Den avgörande verifieringen: signaturen `Sign()` producerar
    /// kontrolleras med `openssl pkeyutl` — HELT OBEROENDE av SSH.NET eller
    /// vår egen kod — ett genuint externt bevis att agent-protokoll-
    /// implementationen faktiskt producerar en giltig Ed25519-signatur
    /// över EXAKT den data som begärdes, inte bara att agenten svarade
    /// utan fel.
    /// </summary>
    [Fact]
    public void SignedDataVerifiesAgainstARealEd25519SignatureCheck()
    {
        if (!HasAgent) return;
        using var agent = SshAgentClient.Connect();
        Assert.NotNull(agent);
        var identities = agent!.RequestIdentities();
        Assert.NotEmpty(identities);
        var (blob, _) = identities[0];

        var data = Encoding.UTF8.GetBytes("bastion-ssh-agent-signing-test-" + Guid.NewGuid());
        var signature = agent.Sign(blob, data);
        Assert.Equal(64, signature.Length); // rå Ed25519-signatur, alltid 64 bytes

        Assert.True(VerifyWithOpenSsl(ExtractRawPublicKey(blob), data, signature), "openssl kunde inte verifiera agentens signatur mot exakt den skickade datan");
    }

    [Fact]
    public void SignatureDoesNotVerifyAgainstTamperedData()
    {
        if (!HasAgent) return;
        using var agent = SshAgentClient.Connect();
        Assert.NotNull(agent);
        var identities = agent!.RequestIdentities();
        Assert.NotEmpty(identities);
        var (blob, _) = identities[0];

        var data = Encoding.UTF8.GetBytes("original-data");
        var signature = agent.Sign(blob, data);
        var tampered = Encoding.UTF8.GetBytes("tampered-data!!");

        Assert.False(VerifyWithOpenSsl(ExtractRawPublicKey(blob), tampered, signature), "en signatur över ANNAN data ska inte verifiera");
    }

    private static string ExtractAlgorithmName(byte[] blob)
    {
        var length = (int)BinaryPrimitives.ReadUInt32BigEndian(blob);
        return Encoding.ASCII.GetString(blob, 4, length);
    }

    private static byte[] ExtractRawPublicKey(byte[] blob)
    {
        var offset = 0;
        var nameLen = (int)BinaryPrimitives.ReadUInt32BigEndian(blob.AsSpan(offset));
        offset += 4 + nameLen;
        var keyLen = (int)BinaryPrimitives.ReadUInt32BigEndian(blob.AsSpan(offset));
        offset += 4;
        return blob[offset..(offset + keyLen)];
    }

    /// <summary>DER-prefixet för en Ed25519 SubjectPublicKeyInfo (algoritm-OID 1.3.101.112) är fast/kort nog att skriva ut rakt av, ingen ASN.1-bibliotek behövs.</summary>
    private static string BuildEd25519PublicKeyPem(byte[] rawKey)
    {
        var prefix = Convert.FromHexString("302a300506032b6570032100");
        var der = prefix.Concat(rawKey).ToArray();
        var base64 = Convert.ToBase64String(der);
        var sb = new StringBuilder();
        sb.AppendLine("-----BEGIN PUBLIC KEY-----");
        for (var i = 0; i < base64.Length; i += 64)
            sb.AppendLine(base64.Substring(i, Math.Min(64, base64.Length - i)));
        sb.AppendLine("-----END PUBLIC KEY-----");
        return sb.ToString();
    }

    private static bool VerifyWithOpenSsl(byte[] rawPublicKey, byte[] data, byte[] signature)
    {
        var pemPath = Path.GetTempFileName();
        var sigPath = Path.GetTempFileName();
        var dataPath = Path.GetTempFileName();
        try
        {
            File.WriteAllText(pemPath, BuildEd25519PublicKeyPem(rawPublicKey));
            File.WriteAllBytes(sigPath, signature);
            File.WriteAllBytes(dataPath, data);

            var psi = new ProcessStartInfo("openssl") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false };
            foreach (var arg in new[] { "pkeyutl", "-verify", "-pubin", "-inkey", pemPath, "-rawin", "-in", dataPath, "-sigfile", sigPath })
                psi.ArgumentList.Add(arg);
            using var process = Process.Start(psi)!;
            process.WaitForExit();
            return process.ExitCode == 0;
        }
        finally
        {
            File.Delete(pemPath);
            File.Delete(sigPath);
            File.Delete(dataPath);
        }
    }
}
