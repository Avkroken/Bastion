using System.Text.Json;
using System.Text.Json.Serialization;

namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/AppSettings.swift (samma fil/fältnamn som
/// LinuxApp/src/settings.rs) — klientbred (inte per värd) inställning för
/// vilka valfria funktionsknappar som visas. Alla standard <c>true</c> så
/// befintliga installationer inte tappar knappar vid uppgradering.
/// </summary>
/// <summary>
/// Immutabel (init-only) med avsikt: <see cref="AppSettingsStore.Current"/>
/// returnerar en referens rakt av (ingen kopiering), och om typen vore
/// muterbar skulle en anropare kunna ändra fälten i det returnerade objektet
/// direkt — vilket också ändrar <see cref="AppSettingsStore"/>s interna
/// tillstånd INNAN <see cref="AppSettingsStore.Update"/> ens anropas, och på
/// så vis kringgår "skriv till disk innan minnet uppdateras"-kontraktet.
/// Uppdatera via <c>current with { ShowDocker = false }</c>, inte mutation.
/// </summary>
public sealed record FeatureToggles
{
    [JsonPropertyName("showDocker")] public bool ShowDocker { get; init; } = true;
    [JsonPropertyName("showSnippets")] public bool ShowSnippets { get; init; } = true;
    [JsonPropertyName("showCommandLibrary")] public bool ShowCommandLibrary { get; init; } = true;
    // Swifts fältnamn är showSFTPBrowser (SFTP versalt) — System.Text.Json
    // gör ingen egen camelCase-omskrivning av redan explicit satta
    // [JsonPropertyName]-namn, så denna rad matchar redan exakt.
    [JsonPropertyName("showSFTPBrowser")] public bool ShowSftpBrowser { get; init; } = true;
    [JsonPropertyName("showPortForward")] public bool ShowPortForward { get; init; } = true;
    [JsonPropertyName("showKeyDeploy")] public bool ShowKeyDeploy { get; init; } = true;
}

/// <summary>Trådsäker persistens för FeatureToggles, `~/.bastion/settings.json`.</summary>
public sealed class AppSettingsStore
{
    private static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };

    private readonly string _path;
    private readonly object _lock = new();
    private FeatureToggles _toggles;

    public static string DefaultPath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".bastion", "settings.json");

    public AppSettingsStore(string? path = null)
    {
        _path = path ?? DefaultPath;
        _toggles = Load(_path);
    }

    /// <summary>
    /// Skiljer "filen finns inte än" (standardvärden är korrekt) från "filen
    /// finns men går inte att tolka" (kasta vidare) — samma princip som
    /// HostStore.Load/LinuxApp:s settings.rs, så en trunkerad/skadad fil inte
    /// tyst kollapsar till standardvärden och sedan skrivs över permanent av
    /// nästa <see cref="Update"/>.
    /// </summary>
    private static FeatureToggles Load(string path)
    {
        if (!File.Exists(path)) return new FeatureToggles();
        return JsonSerializer.Deserialize<FeatureToggles>(File.ReadAllText(path), JsonOptions) ?? new FeatureToggles();
    }

    public FeatureToggles Current()
    {
        lock (_lock) return _toggles;
    }

    /// <summary>
    /// Skriver till disk INNAN toggles-fältet uppdateras — om skrivningen
    /// misslyckas fortsätter Current() returnera föregående värde, annars
    /// hade GUI:t tyst visat ett läge som reverterar efter omstart utan att
    /// användaren fått veta att det aldrig sparades.
    /// </summary>
    public void Update(FeatureToggles newValue)
    {
        lock (_lock)
        {
            Persist(newValue);
            _toggles = newValue;
        }
    }

    private void Persist(FeatureToggles toggles)
    {
        var dir = Path.GetDirectoryName(_path);
        if (!string.IsNullOrEmpty(dir)) Directory.CreateDirectory(dir);
        FsUtil.AtomicWriteText(_path, JsonSerializer.Serialize(toggles, JsonOptions));
    }
}
