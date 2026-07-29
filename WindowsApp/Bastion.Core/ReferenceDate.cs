using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bastion.Core;

/// <summary>
/// Sekunder sedan 2001-01-01T00:00:00Z — samma epok som Swifts
/// Date.timeIntervalSinceReferenceDate, INTE Unix-epok. Verifierat empiriskt
/// mot en riktig swift-körning (se LinuxApp/src/host.rs för samma port).
/// </summary>
[JsonConverter(typeof(ReferenceDateConverter))]
public readonly struct ReferenceDate : IEquatable<ReferenceDate>
{
    private const double UnixOffsetSeconds = 978_307_200.0;

    public double Seconds { get; }

    public ReferenceDate(double seconds) => Seconds = seconds;

    public static ReferenceDate Now()
    {
        var unix = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() / 1000.0;
        return new ReferenceDate(unix - UnixOffsetSeconds);
    }

    public bool Equals(ReferenceDate other) => Seconds.Equals(other.Seconds);
    public override bool Equals(object? obj) => obj is ReferenceDate other && Equals(other);
    public override int GetHashCode() => Seconds.GetHashCode();
}

public sealed class ReferenceDateConverter : JsonConverter<ReferenceDate>
{
    public override ReferenceDate Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
        => new(reader.GetDouble());

    public override void Write(Utf8JsonWriter writer, ReferenceDate value, JsonSerializerOptions options)
        => writer.WriteNumberValue(value.Seconds);
}
