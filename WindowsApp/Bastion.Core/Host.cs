using System.Text.Json.Serialization;

namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/Host.swift — samma fältnamn (camelCase, verbatim
/// Codable-utdata) så filen kan delas med App/ och LinuxApp (host.rs).
/// </summary>
public sealed class Host
{
    [JsonPropertyName("id")] public Guid Id { get; set; } = Guid.NewGuid();
    [JsonPropertyName("alias")] public string Alias { get; set; } = "";
    [JsonPropertyName("hostName")] public string HostName { get; set; } = "";
    [JsonPropertyName("user")] public string User { get; set; } = "";
    [JsonPropertyName("port")] public long Port { get; set; } = 22;
    [JsonPropertyName("tags")] public List<string> Tags { get; set; } = new();
    [JsonPropertyName("auth")] public HostAuth Auth { get; set; } = new HostAuth.AgentDefault();
    [JsonPropertyName("isFavorite")] public bool IsFavorite { get; set; }
    [JsonPropertyName("colorTag")] public string? ColorTag { get; set; }
    [JsonPropertyName("platform")] public RemotePlatform Platform { get; set; } = RemotePlatform.Posix;
    [JsonPropertyName("startupCommand")] public string? StartupCommand { get; set; }
    [JsonPropertyName("jumpHostID")] public Guid? JumpHostId { get; set; }
    [JsonPropertyName("macAddress")] public string? MacAddress { get; set; }
    [JsonPropertyName("modifiedAt")] public ReferenceDate ModifiedAt { get; set; } = ReferenceDate.Now();

    public static Host Create(string alias, string hostName, string user) => new()
    {
        Alias = alias,
        HostName = hostName,
        User = user,
    };
}
