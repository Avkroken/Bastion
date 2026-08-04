using System.Net.Sockets;

namespace Bastion.Core;

/// <summary>
/// SSH-agent-protokollklient (draft-miller-ssh-agent) — WindowsApps
/// motsvarighet till att ssh-agent-baserad autentisering (HostAuth.
/// AgentDefault) fungerar i LinuxApp (via russh::keys::agent) och App/
/// (via SSHCores AgentClient). SSH.NET har INGET inbyggt agent-
/// protokollstöd (dokumenterad, väletablerad begränsning — se SshSession.cs
/// klassdoc) — den här klassen implementerar tråd-protokollet från grunden
/// och kopplas in i SSH.NET via <see cref="IPrivateKeySource"/>/
/// <see cref="Renci.SshNet.Security.HostAlgorithm"/> (se
/// <see cref="AgentPrivateKeySource"/>/<see cref="AgentHostAlgorithm"/>
/// nedan), inte via ett tredjepartsbibliotek.
///
/// KÄND BEGRÄNSNING: bara Ed25519-identiteter stöds (samma prioritering
/// som resten av Bastion — Ed25519 är standardnyckeltypen överallt i
/// projektet). RSA/ECDSA-identiteter i agenten hoppas över tyst av
/// <see cref="RequestIdentities"/>, inte fel — en agent med en blandning
/// av nyckeltyper ska fortfarande fungera för de Ed25519-nycklar den bär.
///
/// Transport: Unix domain socket (`SSH_AUTH_SOCK`, Linux/macOS/WSL) eller
/// en Windows named pipe (`\\.\pipe\openssh-ssh-agent`, Win32-OpenSSHs
/// standardagent) — se <see cref="Connect"/>.
public sealed class SshAgentClient : IDisposable
{
    private const byte RequestIdentitiesMessage = 11;
    private const byte IdentitiesAnswerMessage = 12;
    private const byte SignRequestMessage = 13;
    private const byte SignResponseMessage = 14;
    private const byte FailureMessage = 5;

    private readonly Stream _stream;

    private SshAgentClient(Stream stream) => _stream = stream;

    /// <summary>
    /// Ansluter till den lokala ssh-agenten. `null` om ingen agent hittas
    /// (`SSH_AUTH_SOCK` är inte satt, eller anslutningen misslyckas) —
    /// anroparen faller då tillbaka på att INTE erbjuda agent-baserad
    /// autentisering, samma "tyst frånvarande"-princip som övriga klienter.
    /// </summary>
    public static SshAgentClient? Connect()
    {
        var authSock = Environment.GetEnvironmentVariable("SSH_AUTH_SOCK");
        if (!string.IsNullOrEmpty(authSock))
        {
            try
            {
                var socket = new Socket(AddressFamily.Unix, SocketType.Stream, ProtocolType.Unspecified);
                socket.Connect(new UnixDomainSocketEndPoint(authSock));
                return new SshAgentClient(new NetworkStream(socket, ownsSocket: true));
            }
            catch (SocketException)
            {
                return null;
            }
        }

        if (OperatingSystem.IsWindows())
        {
            try
            {
                var pipe = new System.IO.Pipes.NamedPipeClientStream(
                    ".", "openssh-ssh-agent", System.IO.Pipes.PipeDirection.InOut, System.IO.Pipes.PipeOptions.None);
                pipe.Connect(500);
                return new SshAgentClient(pipe);
            }
            catch (Exception e) when (e is TimeoutException or IOException or UnauthorizedAccessException)
            {
                return null;
            }
        }

        return null;
    }

    /// <summary>Ed25519-identiteter agenten för närvarande har laddade — `(PublicKeyBlob, Comment)` per identitet.</summary>
    public IReadOnlyList<(byte[] PublicKeyBlob, string Comment)> RequestIdentities()
    {
        WriteFrame(new byte[] { RequestIdentitiesMessage });
        var (type, payload) = ReadFrame();
        if (type != IdentitiesAnswerMessage) return [];

        // Samma gräns som OpenSSH sätter för antal identiteter i ett svar —
        // förhindrar att en trasig/skadlig `count` (t.ex. 0xFFFFFFFF)
        // orsakar en jättelik `List<>`-preallokering (CodeRabbit-fynd).
        const uint maxIdentities = 2048;
        var reader = new SshWireReader(payload);
        var count = reader.ReadUInt32();
        if (count > maxIdentities)
        {
            throw new IOException($"ssh-agent uppgav ett orimligt antal identiteter ({count})");
        }
        var result = new List<(byte[], string)>((int)count);
        for (var i = 0; i < count; i++)
        {
            var blob = reader.ReadString();
            var comment = System.Text.Encoding.UTF8.GetString(reader.ReadString());
            // Bara Ed25519 stöds (se klassdoc) — nyckeltypen är de första
            // fyra byten av blobben, en SSH-wire-sträng ("ssh-ed25519").
            if (KeyBlobAlgorithmName(blob) == "ssh-ed25519")
            {
                result.Add((blob, comment));
            }
        }
        return result;
    }

    /// <summary>
    /// Ber agenten signera `data` med den privata nyckeln som hör till
    /// `publicKeyBlob` (måste vara en blob agenten faktiskt har laddad,
    /// t.ex. från <see cref="RequestIdentities"/>). Returnerar RÅ
    /// signaturbytes (INTE den SSH-formaterade `sig_format || sig_blob`-
    /// kuverten agenten själv svarar med — den packas upp här, eftersom
    /// SSH.NETs <see cref="Renci.SshNet.Security.HostAlgorithm.Sign"/>
    /// förväntas returnera rå signaturdata som SSH.NET sedan slår in i sitt
    /// EGET SSH_MSG_USERAUTH_REQUEST-kuvert).
    /// </summary>
    public byte[] Sign(byte[] publicKeyBlob, byte[] data)
    {
        var writer = new SshWireWriter();
        writer.WriteString(publicKeyBlob);
        writer.WriteString(data);
        writer.WriteUInt32(0); // inga flaggor — bara Ed25519 stöds, som saknar SHA2-varianter (RFC 8332 gäller bara RSA)

        var request = new byte[1 + writer.Length];
        request[0] = SignRequestMessage;
        writer.CopyTo(request.AsSpan(1));
        WriteFrame(request);

        var (type, payload) = ReadFrame();
        if (type != SignResponseMessage)
        {
            throw new InvalidOperationException("ssh-agent avvisade signeringsbegäran (SSH_AGENT_FAILURE)");
        }
        var reader = new SshWireReader(payload);
        var signatureBlob = reader.ReadString();
        // signatureBlob är i sig en SSH-wire-sträng: format-namn + rå signatur.
        var inner = new SshWireReader(signatureBlob);
        var format = System.Text.Encoding.ASCII.GetString(inner.ReadString());
        var rawSignature = inner.ReadString();
        if (format != "ssh-ed25519")
        {
            throw new InvalidOperationException($"ssh-agent svarade med ett oväntat signaturformat: {format}");
        }
        return rawSignature;
    }

    private static string KeyBlobAlgorithmName(byte[] blob)
    {
        var reader = new SshWireReader(blob);
        return System.Text.Encoding.ASCII.GetString(reader.ReadString());
    }

    private void WriteFrame(byte[] payload)
    {
        var lengthPrefix = new byte[4];
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(lengthPrefix, (uint)payload.Length);
        _stream.Write(lengthPrefix);
        _stream.Write(payload);
        _stream.Flush();
    }

    // Samma gräns som OpenSSHs egen ssh-agent-klient sätter för ett svar —
    // en lokal agent ska ALDRIG svara med mer än detta, så en längre
    // uppgiven ram är ett tecken på ett trasigt/skadligt svar, inte ett
    // legitimt stort paket (CodeRabbit-fynd: en obegränsad `(int)length`-
    // cast innan allokering kunde annars kastas ett `OverflowException`/
    // `OutOfMemoryException` istället för ett tydligt protokollfel).
    private const int MaxReplyBytes = 256 * 1024;

    private (byte Type, byte[] Payload) ReadFrame()
    {
        var lengthPrefix = ReadExact(4);
        var length = System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(lengthPrefix);
        if (length == 0)
        {
            return (FailureMessage, []);
        }
        if (length > MaxReplyBytes)
        {
            throw new IOException($"ssh-agent-svaret uppgav en orimligt stor ramlängd ({length} byte) — avvisat som ett trasigt/skadligt svar");
        }
        var body = ReadExact((int)length);
        return (body[0], body[1..]);
    }

    private byte[] ReadExact(int count)
    {
        var buffer = new byte[count];
        var offset = 0;
        while (offset < count)
        {
            var read = _stream.Read(buffer, offset, count - offset);
            if (read == 0) throw new IOException("ssh-agent stängde anslutningen oväntat");
            offset += read;
        }
        return buffer;
    }

    public void Dispose() => _stream.Dispose();
}

/// <summary>
/// Läser SSH-protokollets grundläggande wire-typer (uint32, längdprefixad
/// sträng) ur en bytebuffert. Kontrollerar ÅTERSTÅENDE längd innan varje
/// läsning/slice — ett trasigt/skadligt agent-svar med en uppgiven
/// stränglängd längre än vad som faktiskt finns kvar i bufferten hade
/// annars kastat ett ospecifikt `ArgumentOutOfRangeException` från
/// span-indexeringen i stället för ett tydligt protokollfel
/// (CodeRabbit-fynd).
/// </summary>
internal ref struct SshWireReader(ReadOnlySpan<byte> data)
{
    private ReadOnlySpan<byte> _data = data;

    public uint ReadUInt32()
    {
        if (_data.Length < 4)
        {
            throw new IOException("ssh-agent-svaret klipptes av mitt i ett uint32-fält");
        }
        var value = System.Buffers.Binary.BinaryPrimitives.ReadUInt32BigEndian(_data);
        _data = _data[4..];
        return value;
    }

    public byte[] ReadString()
    {
        var length = ReadUInt32();
        if (length > (uint)_data.Length)
        {
            throw new IOException("ssh-agent-svaret uppgav en strängängd längre än återstående data");
        }
        var value = _data[..(int)length].ToArray();
        _data = _data[(int)length..];
        return value;
    }
}

/// <summary>Skriver SSH-protokollets grundläggande wire-typer till en växande buffert.</summary>
internal sealed class SshWireWriter
{
    private readonly MemoryStream _buffer = new();

    public int Length => (int)_buffer.Length;

    public void WriteUInt32(uint value)
    {
        Span<byte> bytes = stackalloc byte[4];
        System.Buffers.Binary.BinaryPrimitives.WriteUInt32BigEndian(bytes, value);
        _buffer.Write(bytes);
    }

    public void WriteString(byte[] value)
    {
        WriteUInt32((uint)value.Length);
        _buffer.Write(value);
    }

    public void CopyTo(Span<byte> destination) => _buffer.ToArray().CopyTo(destination);
}

/// <summary>
/// <see cref="Renci.SshNet.Security.HostAlgorithm"/>-omslag runt en enskild
/// ssh-agent-identitet — delegerar signering till agenten istället för att
/// hålla den privata nyckeln själv (den lämnar aldrig agentprocessen).
/// </summary>
public sealed class AgentHostAlgorithm(SshAgentClient agent, byte[] publicKeyBlob) : Renci.SshNet.Security.HostAlgorithm("ssh-ed25519")
{
    public override byte[] Data => publicKeyBlob;

    /// <summary>
    /// SSH.NET tilldelar `Sign`s returvärde RAKT AV till
    /// `RequestMessagePublicKey.Signature` (se t.ex.
    /// `KeyHostAlgorithm.Sign` i SSH.NET-källan) — det förväntas alltså
    /// redan vara den SSH-KODADE signatur-bloben (format-namn +
    /// signaturbytes, båda som wire-strängar), INTE de råa signatur-
    /// bytesen `SshAgentClient.Sign` returnerar. Att skicka de råa bytesen
    /// direkt (tidigare bugg — CodeRabbit-fynd) hade gett en ogiltig
    /// USERAUTH_REQUEST-signatur och agent-autentisering hade ALDRIG
    /// lyckats, trots att både agent-protokollet och SSH.NET-anropet såg
    /// rätt ut var för sig.
    /// </summary>
    public override byte[] Sign(byte[] data)
    {
        var rawSignature = agent.Sign(publicKeyBlob, data);
        var writer = new SshWireWriter();
        writer.WriteString(System.Text.Encoding.ASCII.GetBytes(Name));
        writer.WriteString(rawSignature);
        var signature = new byte[writer.Length];
        writer.CopyTo(signature);
        return signature;
    }

    /// <summary>
    /// Aldrig anropad i den KLIENTSIDA-autentiseringsvägen (SSH.NET
    /// verifierar ALDRIG sin egen nyss skapade signatur) — bara relevant
    /// för att verifiera FJÄRRSIDANS värdnyckelsignaturer under
    /// nyckelutbytet, vilket sköts av en annan HostAlgorithm-instans.
    /// </summary>
    public override bool VerifySignature(byte[] data, byte[] signature) =>
        throw new NotSupportedException("VerifySignature anropas aldrig för en klientsidans agent-autentiseringsidentitet");
}

/// <summary>
/// <see cref="IPrivateKeySource"/>-omslag som exponerar EN ssh-agent-
/// identitet till <see cref="Renci.SshNet.PrivateKeyAuthenticationMethod"/>
/// — en per identitet, agenten kan ha flera laddade.
/// </summary>
public sealed class AgentPrivateKeySource : Renci.SshNet.IPrivateKeySource
{
    public IReadOnlyCollection<Renci.SshNet.Security.HostAlgorithm> HostKeyAlgorithms { get; }

    public AgentPrivateKeySource(SshAgentClient agent, byte[] publicKeyBlob) =>
        HostKeyAlgorithms = [new AgentHostAlgorithm(agent, publicKeyBlob)];
}
