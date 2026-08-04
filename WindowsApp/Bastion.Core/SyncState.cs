using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bastion.Core;

/// <summary>
/// Port av SyncEngine.SyncState. `Tombstones` kodas som en PLATT ARRAY
/// (`[id1, date1, id2, date2, ...]`), inte ett JSON-objekt — UUID är ingen
/// giltig Codable-objektnyckel i Swift. Verifierat empiriskt, se
/// LinuxApp/src/host.rs för samma port.
/// </summary>
[JsonConverter(typeof(SyncStateConverter))]
public sealed class SyncState
{
    public List<Host> Hosts { get; set; } = new();
    public Dictionary<Guid, ReferenceDate> Tombstones { get; set; } = new();
}

public sealed class SyncStateConverter : JsonConverter<SyncState>
{
    /// <summary>
    /// `HostStore.Load` faller tillbaka på ett äldre format (en ren
    /// <c>Host[]</c>-array) om detta kastar `JsonException` — men
    /// `JsonElement.GetProperty`/`GetString`/`GetDouble`/`Guid.Parse` kastar
    /// ALLA andra otypade undantag (`KeyNotFoundException`,
    /// `InvalidOperationException`, `FormatException`) på precis den
    /// legacy-arrayen (rot = array, ingen `"hosts"`-egenskap), vilket
    /// tidigare lät felet propagera FÖRBI fallback-vägen istället för att
    /// trigga den (CodeRabbit-fynd). Allt nedan är därför wrappat så ATT
    /// undantaget alltid blir en `JsonException`.
    /// </summary>
    public override SyncState Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options)
    {
        using var doc = JsonDocument.ParseValue(ref reader);
        var root = doc.RootElement;
        try
        {
            if (root.ValueKind != JsonValueKind.Object)
                throw new JsonException($"SyncState: väntade ett JSON-objekt, fick {root.ValueKind}");
            if (!root.TryGetProperty("hosts", out var hostsElement))
                throw new JsonException("SyncState: saknar fältet hosts");
            if (!root.TryGetProperty("tombstones", out var flat))
                throw new JsonException("SyncState: saknar fältet tombstones");

            var state = new SyncState
            {
                Hosts = JsonSerializer.Deserialize<List<Host>>(hostsElement.GetRawText(), options) ?? new(),
            };
            var items = flat.EnumerateArray().ToList();
            if (items.Count % 2 != 0)
                throw new JsonException("tombstones: udda antal element i platt array");
            for (var i = 0; i < items.Count; i += 2)
            {
                var id = Guid.Parse(items[i].GetString() ?? throw new JsonException("tombstones: ogiltig UUID-nyckel"));
                state.Tombstones[id] = new ReferenceDate(items[i + 1].GetDouble());
            }
            return state;
        }
        catch (Exception e) when (e is not JsonException)
        {
            throw new JsonException($"SyncState: {e.Message}", e);
        }
    }

    public override void Write(Utf8JsonWriter writer, SyncState value, JsonSerializerOptions options)
    {
        writer.WriteStartObject();
        writer.WritePropertyName("hosts");
        JsonSerializer.Serialize(writer, value.Hosts, options);
        writer.WritePropertyName("tombstones");
        writer.WriteStartArray();
        foreach (var (id, date) in value.Tombstones)
        {
            writer.WriteStringValue(id.ToString());
            writer.WriteNumberValue(date.Seconds);
        }
        writer.WriteEndArray();
        writer.WriteEndObject();
    }
}
