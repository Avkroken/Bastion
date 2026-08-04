using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;

namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/Snippet.swift (samma design som LinuxApp/src/snippet.rs).
/// Ett sparat kommando med `{{variabel}}`-mall — inte bara text, kan fyllas i
/// per körning (VISION.md: "Restart Plex → ssh → docker compose restart plex").
/// </summary>
public sealed partial class Snippet
{
    [JsonPropertyName("id")] public Guid Id { get; set; } = Guid.NewGuid();
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("template")] public string Template { get; set; } = "";
    [JsonPropertyName("modifiedAt")] public ReferenceDate ModifiedAt { get; set; } = ReferenceDate.Now();

    public static Snippet Create(string name, string template) => new() { Name = name, Template = template };

    [GeneratedRegex(@"\{\{\s*([^{}]*?)\s*\}\}")]
    private static partial Regex VariablePattern();

    /// <summary>Hittar varje {{namn}}-förekomst, mellanslag runt namnet trimmas, i den ordning de står.</summary>
    private List<Match> Occurrences() =>
        VariablePattern().Matches(Template).Cast<Match>().Where(m => m.Groups[1].Value.Length > 0).ToList();

    /// <summary>Variabelnamnen i mallen, i första-förekomst-ordning, utan dubbletter.</summary>
    public IReadOnlyList<string> VariableNames()
    {
        var seen = new HashSet<string>();
        return Occurrences().Select(m => m.Groups[1].Value).Where(n => seen.Add(n)).ToList();
    }

    /// <summary>
    /// Ersätter varje {{namn}}-förekomst med values[namn]. Saknade värden blir
    /// tom sträng — en halvifylld snippet är fortfarande ett giltigt, om än
    /// ofullständigt, kommando att granska innan det skickas.
    /// </summary>
    public string Rendered(IReadOnlyDictionary<string, string> values)
    {
        var result = new System.Text.StringBuilder();
        var lastEnd = 0;
        foreach (var m in Occurrences())
        {
            result.Append(Template, lastEnd, m.Index - lastEnd);
            result.Append(values.TryGetValue(m.Groups[1].Value, out var v) ? v : "");
            lastEnd = m.Index + m.Length;
        }
        result.Append(Template, lastEnd, Template.Length - lastEnd);
        return result.ToString();
    }
}

/// <summary>
/// Persistent snippet-databas, `~/.bastion/snippets.json` — samma mönster
/// som HostStore men en ren array (ingen synk-integration, se ROADMAP.md).
/// </summary>
public sealed class SnippetStore
{
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    private readonly string _path;
    private List<Snippet> _snippets;
    private readonly object _lock = new();

    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".bastion", "snippets.json");

    public SnippetStore(string path)
    {
        _path = path;
        _snippets = Load(path);
    }

    /// <summary>
    /// Skiljer "filen finns inte än" (tom lista är korrekt) från "filen
    /// finns men går inte att tolka" (kasta vidare) — samma princip som
    /// HostStore.Load, så en trunkerad/skadad fil inte tyst blir en tom
    /// lista som sedan skrivs över permanent av nästa <see cref="Upsert"/>.
    /// </summary>
    private static List<Snippet> Load(string path)
    {
        if (!File.Exists(path)) return new();
        return JsonSerializer.Deserialize<List<Snippet>>(File.ReadAllText(path), JsonOptions) ?? new();
    }

    public List<Snippet> All()
    {
        lock (_lock)
            return _snippets.OrderBy(s => s.Name, StringComparer.OrdinalIgnoreCase).ToList();
    }

    public void Upsert(Snippet snippet)
    {
        lock (_lock)
        {
            snippet.ModifiedAt = ReferenceDate.Now();
            var index = _snippets.FindIndex(s => s.Id == snippet.Id);
            if (index >= 0) _snippets[index] = snippet;
            else _snippets.Add(snippet);
            Persist();
        }
    }

    public void Delete(Guid id)
    {
        lock (_lock)
        {
            _snippets.RemoveAll(s => s.Id == id);
            Persist();
        }
    }

    private void Persist()
    {
        var dir = Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        var sorted = _snippets.OrderBy(s => s.Name, StringComparer.Ordinal).ToList();
        FsUtil.AtomicWriteText(_path, JsonSerializer.Serialize(sorted, JsonOptions));
    }
}
