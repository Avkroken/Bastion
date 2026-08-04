using Bastion.Core;
using Xunit;

namespace Bastion.Core.Tests;

public class SnippetTests
{
    [Fact]
    public void VariableNamesAreInFirstOccurrenceOrderWithoutDuplicates()
    {
        var s = Snippet.Create("t", "docker compose restart {{service}} && journalctl -u {{service}} -n {{n}}");
        Assert.Equal(new[] { "service", "n" }, s.VariableNames());
    }

    [Fact]
    public void RenderedSubstitutesValuesAndTrimsWhitespaceInNames()
    {
        var s = Snippet.Create("t", "restart {{ service }}");
        var values = new Dictionary<string, string> { ["service"] = "web" };
        Assert.Equal("restart web", s.Rendered(values));
    }

    [Fact]
    public void MissingValuesRenderAsEmptyStringNotLeftAsPlaceholder()
    {
        var s = Snippet.Create("t", "restart {{service}}");
        Assert.Equal("restart ", s.Rendered(new Dictionary<string, string>()));
    }

    [Fact]
    public void StoreRoundTripsThroughDisk()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-snippet-test-{Guid.NewGuid()}");
        var path = Path.Combine(dir, "snippets.json");
        try
        {
            var store = new SnippetStore(path);
            store.Upsert(Snippet.Create("Restart web", "docker compose restart {{service}}"));

            var reopened = new SnippetStore(path);
            Assert.Single(reopened.All());
            Assert.Equal("Restart web", reopened.All()[0].Name);
        }
        finally
        {
            if (Directory.Exists(dir)) Directory.Delete(dir, recursive: true);
        }
    }

    [Fact]
    public void DeleteRemovesSnippetAndPersists()
    {
        var dir = Path.Combine(Path.GetTempPath(), $"bastion-snippet-test-{Guid.NewGuid()}");
        var path = Path.Combine(dir, "snippets.json");
        try
        {
            var store = new SnippetStore(path);
            var snippet = Snippet.Create("Restart web", "docker compose restart {{service}}");
            store.Upsert(snippet);
            store.Delete(snippet.Id);

            var reopened = new SnippetStore(path);
            Assert.Empty(reopened.All());
        }
        finally
        {
            if (Directory.Exists(dir)) Directory.Delete(dir, recursive: true);
        }
    }
}

public class CommandLibraryTests
{
    [Fact]
    public void AllEntriesHaveNonEmptyCommandAndSummary()
    {
        foreach (var entry in CommandLibrary.All)
        {
            Assert.False(string.IsNullOrWhiteSpace(entry.Command));
            Assert.False(string.IsNullOrWhiteSpace(entry.Summary));
        }
    }

    [Fact]
    public void EntriesFiltersByCategory()
    {
        var dockerEntries = CommandLibrary.Entries(CommandLibraryCategory.Docker).ToList();
        Assert.NotEmpty(dockerEntries);
        Assert.All(dockerEntries, e => Assert.Equal(CommandLibraryCategory.Docker, e.Category));
    }

    [Fact]
    public void AsSnippetRendersVariablesLikeARegularSnippet()
    {
        var entry = CommandLibrary.Entries(CommandLibraryCategory.Docker)
            .First(e => e.Command.Contains("{{service}}") && e.Command.Contains("restart"));
        var rendered = entry.AsSnippet.Rendered(new Dictionary<string, string> { ["service"] = "plex" });
        Assert.Equal("docker compose restart plex", rendered);
    }

    [Fact]
    public void IdsAreUniqueAcrossTheWholeLibrary()
    {
        var ids = CommandLibrary.All.Select(e => e.Id).ToList();
        Assert.Equal(ids.Count, ids.Distinct().Count());
    }
}
