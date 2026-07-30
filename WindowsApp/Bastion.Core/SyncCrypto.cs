using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/SyncCrypto.swift (samma "BSYNC1"-kuvertformat som
/// LinuxApp/src/sync_crypto.rs): PBKDF2-HMAC-SHA256 nyckelderivering +
/// AES-256-GCM. Kuvert: magic(6) || iterations u32 big-endian(4) ||
/// salt(16) || nonce(12) || ciphertext || tag(16) — identiskt med Apple
/// CryptoKits AES.GCM.SealedBox.combined-layout, cross-språkverifierat
/// byte-för-byte mot både Swift och Rust.
/// </summary>
public static class SyncCrypto
{
    public const int DefaultIterations = 210_000;
    private static readonly byte[] Magic = Encoding.ASCII.GetBytes("BSYNC1");
    private const int SaltLen = 16;
    private const int NonceLen = 12;
    private const int TagLen = 16;

    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    private static byte[] DeriveKey(string passphrase, byte[] salt, int iterations) =>
        Rfc2898DeriveBytes.Pbkdf2(Encoding.UTF8.GetBytes(passphrase), salt, iterations, HashAlgorithmName.SHA256, 32);

    public static byte[] Seal(SyncState state, string passphrase, int iterations = DefaultIterations)
    {
        var salt = RandomNumberGenerator.GetBytes(SaltLen);
        var nonce = RandomNumberGenerator.GetBytes(NonceLen);
        var key = DeriveKey(passphrase, salt, iterations);

        var plaintext = JsonSerializer.SerializeToUtf8Bytes(state, JsonOptions);
        var ciphertext = new byte[plaintext.Length];
        var tag = new byte[TagLen];
        using (var aes = new AesGcm(key, TagLen))
        {
            aes.Encrypt(nonce, plaintext, ciphertext, tag);
        }

        var iterationsBytes = BitConverter.GetBytes((uint)iterations);
        if (BitConverter.IsLittleEndian) Array.Reverse(iterationsBytes);

        using var output = new MemoryStream();
        output.Write(Magic);
        output.Write(iterationsBytes);
        output.Write(salt);
        output.Write(nonce);
        output.Write(ciphertext);
        output.Write(tag);
        return output.ToArray();
    }

    public static SyncState Open(byte[] data, string passphrase)
    {
        var headerLen = Magic.Length + 4 + SaltLen + NonceLen;
        if (data.Length <= headerLen + TagLen || !data.AsSpan(0, Magic.Length).SequenceEqual(Magic))
            throw new SyncCryptoException(SyncCryptoError.BadFormat);

        var iterationsBytes = data.AsSpan(Magic.Length, 4).ToArray();
        if (BitConverter.IsLittleEndian) Array.Reverse(iterationsBytes);
        var iterations = (int)BitConverter.ToUInt32(iterationsBytes);

        var salt = data.AsSpan(Magic.Length + 4, SaltLen).ToArray();
        var nonce = data.AsSpan(Magic.Length + 4 + SaltLen, NonceLen).ToArray();
        var ciphertext = data.AsSpan(headerLen, data.Length - headerLen - TagLen).ToArray();
        var tag = data.AsSpan(data.Length - TagLen, TagLen).ToArray();

        var key = DeriveKey(passphrase, salt, iterations);
        var plaintext = new byte[ciphertext.Length];
        try
        {
            using var aes = new AesGcm(key, TagLen);
            aes.Decrypt(nonce, ciphertext, tag, plaintext);
        }
        catch (CryptographicException)
        {
            throw new SyncCryptoException(SyncCryptoError.WrongPassphraseOrTampered);
        }

        try
        {
            return JsonSerializer.Deserialize<SyncState>(plaintext) ?? throw new SyncCryptoException(SyncCryptoError.WrongPassphraseOrTampered);
        }
        catch (JsonException)
        {
            throw new SyncCryptoException(SyncCryptoError.WrongPassphraseOrTampered);
        }
    }
}

public enum SyncCryptoError { BadFormat, WrongPassphraseOrTampered }

public sealed class SyncCryptoException(SyncCryptoError error) : Exception(error.ToString())
{
    public SyncCryptoError Error { get; } = error;
}

/// <summary>
/// Port av EncryptedFolderSyncProvider — rätt för en tredjeparts molnmapp
/// (Dropbox/Drive/OneDrive) man inte litar på blint. Lösenfrasen sparas
/// aldrig på disk, bara i minnet under ett synk-anrop.
/// </summary>
public sealed class EncryptedFolderSyncProvider(string path, string passphrase) : ISyncProvider
{
    public SyncState? Pull()
    {
        if (!File.Exists(path)) return null;
        return SyncCrypto.Open(File.ReadAllBytes(path), passphrase);
    }

    public void Push(SyncState state)
    {
        var dir = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        File.WriteAllBytes(path, SyncCrypto.Seal(state, passphrase));
    }
}
