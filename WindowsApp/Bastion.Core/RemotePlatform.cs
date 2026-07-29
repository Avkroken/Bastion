using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bastion.Core;

/// <summary>Port av RemotePlatform (String-rawValue-enum, kodas som en vanlig JSON-sträng).</summary>
[JsonConverter(typeof(RemotePlatformConverter))]
public enum RemotePlatform
{
    Posix,
    WindowsAdmin,
    WindowsStandard,
}

public sealed class RemotePlatformConverter : JsonConverter<RemotePlatform>
{
    public override RemotePlatform Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) =>
        reader.GetString() switch
        {
            "posix" => RemotePlatform.Posix,
            "windowsAdmin" => RemotePlatform.WindowsAdmin,
            "windowsStandard" => RemotePlatform.WindowsStandard,
            var other => throw new JsonException($"RemotePlatform: okänt värde {other}"),
        };

    public override void Write(Utf8JsonWriter writer, RemotePlatform value, JsonSerializerOptions options) =>
        writer.WriteStringValue(value switch
        {
            RemotePlatform.Posix => "posix",
            RemotePlatform.WindowsAdmin => "windowsAdmin",
            RemotePlatform.WindowsStandard => "windowsStandard",
            _ => throw new JsonException($"RemotePlatform: okänt värde {value}"),
        });
}
