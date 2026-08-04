using System.Text.Json;
using Bastion.Core;

namespace Bastion.Core.Tests;

public class HostAuthTests
{
    [Fact]
    public void KeyFileRoundTripsAndMatchesSwiftWireFormat()
    {
        var auth = new HostAuth.KeyFile("/x/y");
        var json = JsonSerializer.Serialize<HostAuth>(auth);
        Assert.Equal("""{"keyFile":{"_0":"/x/y"}}""", json);
        var back = JsonSerializer.Deserialize<HostAuth>(json);
        Assert.Equal(auth, back);
    }

    [Fact]
    public void AskPasswordHasNoPayload()
    {
        var json = JsonSerializer.Serialize<HostAuth>(new HostAuth.AskPassword());
        Assert.Equal("""{"askPassword":{}}""", json);
    }

    [Fact]
    public void CertificateFileRoundTrips()
    {
        var auth = new HostAuth.CertificateFile("/k", "/c");
        var json = JsonSerializer.Serialize<HostAuth>(auth);
        var back = JsonSerializer.Deserialize<HostAuth>(json);
        Assert.Equal(auth, back);
    }

    [Fact]
    public void MoreThanOneCaseKeyIsRejected()
    {
        Assert.Throws<JsonException>(() => JsonSerializer.Deserialize<HostAuth>("""{"askPassword":{},"agentDefault":{}}"""));
    }

    [Fact]
    public void ANonObjectValueIsRejectedAsAJsonExceptionNotAnUnhandledOne()
    {
        // Måste bli JsonException specifikt — HostStore.Load fångar bara
        // just den typen för att falla tillbaka på äldre format.
        Assert.Throws<JsonException>(() => JsonSerializer.Deserialize<HostAuth>("\"not-an-object\""));
        Assert.Throws<JsonException>(() => JsonSerializer.Deserialize<HostAuth>("42"));
    }
}
