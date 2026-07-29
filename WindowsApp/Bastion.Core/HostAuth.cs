using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/Host.swift:HostAuth. Wire-format verifierat
/// empiriskt mot en riktig swift-körning (se LinuxApp/src/host.rs för
/// samma verifiering): ett enum-case med associerat värde kodas som
/// {"case": {"_0": v}} (omärkt) eller {"case": {"label": v}} (märkt),
/// {"case": {}} utan payload.
/// </summary>
[JsonConverter(typeof(HostAuthConverter))]
public abstract record HostAuth
{
    public sealed record AskPassword : HostAuth;
    public sealed record KeyFile(string Path) : HostAuth;
    public sealed record AgentDefault : HostAuth;
    public sealed record KeychainKey(string Id) : HostAuth;
    public sealed record CertificateFile(string KeyPath, string CertPath) : HostAuth;
    public sealed record BitwardenItem(string Id) : HostAuth;
}

public sealed class HostAuthConverter : JsonConverter<HostAuth>
{
    public override HostAuth Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        using var doc = JsonDocument.ParseValue(ref reader);
        var prop = doc.RootElement.EnumerateObject().FirstOrDefault();
        if (prop.Value.ValueKind == JsonValueKind.Undefined)
            throw new JsonException("HostAuth: tom map, väntade exakt en case-nyckel");
        var payload = prop.Value;
        return prop.Name switch
        {
            "askPassword" => new HostAuth.AskPassword(),
            "agentDefault" => new HostAuth.AgentDefault(),
            "keyFile" => new HostAuth.KeyFile(Field(payload, "_0")),
            "keychainKey" => new HostAuth.KeychainKey(Field(payload, "_0")),
            "bitwardenItem" => new HostAuth.BitwardenItem(Field(payload, "_0")),
            "certificateFile" => new HostAuth.CertificateFile(Field(payload, "keyPath"), Field(payload, "certPath")),
            var other => throw new JsonException($"HostAuth: okänd case {other}"),
        };
    }

    private static string Field(JsonElement payload, string name) =>
        payload.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String
            ? v.GetString()!
            : throw new JsonException($"HostAuth: saknar fältet {name}");

    public override void Write(Utf8JsonWriter writer, HostAuth value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        switch (value)
        {
            case HostAuth.AskPassword:
                writer.WriteStartObject("askPassword");
                writer.WriteEndObject();
                break;
            case HostAuth.AgentDefault:
                writer.WriteStartObject("agentDefault");
                writer.WriteEndObject();
                break;
            case HostAuth.KeyFile kf:
                writer.WriteStartObject("keyFile");
                writer.WriteString("_0", kf.Path);
                writer.WriteEndObject();
                break;
            case HostAuth.KeychainKey kk:
                writer.WriteStartObject("keychainKey");
                writer.WriteString("_0", kk.Id);
                writer.WriteEndObject();
                break;
            case HostAuth.BitwardenItem bw:
                writer.WriteStartObject("bitwardenItem");
                writer.WriteString("_0", bw.Id);
                writer.WriteEndObject();
                break;
            case HostAuth.CertificateFile cf:
                writer.WriteStartObject("certificateFile");
                writer.WriteString("keyPath", cf.KeyPath);
                writer.WriteString("certPath", cf.CertPath);
                writer.WriteEndObject();
                break;
            default:
                throw new JsonException($"HostAuth: okänd underklass {value.GetType()}");
        }
        writer.WriteEndObject();
    }
}
