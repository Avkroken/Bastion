using System.Text;
using Bastion.Core;

namespace Bastion.Core.Tests;

public class SyncCryptoTests
{
    private static string TempDir() => Path.Combine(Path.GetTempPath(), $"bastion-cs-enc-test-{Guid.NewGuid()}");

    private static string Hex(byte[] bytes) => Convert.ToHexString(bytes).ToLowerInvariant();

    // Kända testvektorer för PBKDF2-HMAC-SHA256 (password="password", salt="salt"),
    // identiska med Tests/SSHCoreTests/SyncCryptoTests.swift::testPBKDF2KnownAnswerVectors.
    [Theory]
    [InlineData(1, "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b")]
    [InlineData(2, "ae4d0c95af6b46d32d0adff928f06dd02a303f8ef3c251dfd6e2d85a95474c43")]
    [InlineData(4096, "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a")]
    public void Pbkdf2KnownAnswerVectorsMatchSwiftAndRustReference(int iterations, string expectedHex)
    {
        var key = System.Security.Cryptography.Rfc2898DeriveBytes.Pbkdf2(
            Encoding.UTF8.GetBytes("password"), Encoding.UTF8.GetBytes("salt"), iterations,
            System.Security.Cryptography.HashAlgorithmName.SHA256, 32);
        Assert.Equal(expectedHex, Hex(key));
    }

    private static SyncState SampleState() => new()
    {
        Hosts = [Host.Create("web", "10.0.0.5", "deploy")],
    };

    [Fact]
    public void SealOpenRoundTrips()
    {
        var blob = SyncCrypto.Seal(SampleState(), "correct horse", 1000);
        var opened = SyncCrypto.Open(blob, "correct horse");
        Assert.Equal("web", opened.Hosts[0].Alias);
    }

    [Fact]
    public void WrongPassphraseFails()
    {
        var blob = SyncCrypto.Seal(SampleState(), "rätt", 1000);
        var ex = Assert.Throws<SyncCryptoException>(() => SyncCrypto.Open(blob, "fel"));
        Assert.Equal(SyncCryptoError.WrongPassphraseOrTampered, ex.Error);
    }

    [Fact]
    public void TamperedCiphertextFails()
    {
        var blob = SyncCrypto.Seal(SampleState(), "pw", 1000);
        blob[^1] ^= 0xFF;
        var ex = Assert.Throws<SyncCryptoException>(() => SyncCrypto.Open(blob, "pw"));
        Assert.Equal(SyncCryptoError.WrongPassphraseOrTampered, ex.Error);
    }

    [Fact]
    public void BadFormatIsRejectedCleanly()
    {
        var ex = Assert.Throws<SyncCryptoException>(() => SyncCrypto.Open(Encoding.UTF8.GetBytes("junk"), "pw"));
        Assert.Equal(SyncCryptoError.BadFormat, ex.Error);
    }

    [Fact]
    public void CiphertextLeaksNoPlaintext()
    {
        var blob = SyncCrypto.Seal(SampleState(), "pw", 1000);
        var text = Encoding.UTF8.GetString(blob);
        Assert.DoesNotContain("10.0.0.5", text);
        Assert.DoesNotContain("deploy", text);
        Assert.DoesNotContain("web", text);
    }

    [Fact]
    public void EncryptedProviderConvergesAndRejectsWrongPassphrase()
    {
        var dir = TempDir();
        try
        {
            var sharedPath = Path.Combine(dir, "shared.enc");
            var provider = new EncryptedFolderSyncProvider(sharedPath, "delad-hemlis");

            var deviceA = new HostStore(Path.Combine(dir, "a.json"));
            var deviceB = new HostStore(Path.Combine(dir, "b.json"));

            var host = Host.Create("nas", "10.0.0.2", "root");
            deviceA.Upsert(host);
            deviceA.Sync(provider);
            deviceB.Sync(provider);

            Assert.Equal("nas", deviceB.All().Single(h => h.Id == host.Id).Alias);

            var wrong = new EncryptedFolderSyncProvider(sharedPath, "gissning");
            Assert.Throws<SyncCryptoException>(() => wrong.Pull());
        }
        finally
        {
            Directory.Delete(dir, true);
        }
    }
}
