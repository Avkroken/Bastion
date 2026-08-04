using Bastion.Core;
using Xunit;

namespace Bastion.Core.Tests;

public class DockerServiceTests
{
    [Theory]
    [InlineData("plex")]
    [InlineData("a1b2c3d4e5f6")]
    [InlineData("my_app.1")]
    [InlineData("web-1")]
    [InlineData("Radarr")]
    public void ValidateAcceptsRealReferences(string ok) =>
        Assert.Equal(ok, DockerService.Validate(ok));

    [Theory]
    [InlineData("plex; rm -rf /")]
    [InlineData("a b")]
    [InlineData("$(whoami)")]
    [InlineData("`id`")]
    [InlineData("a|b")]
    [InlineData("a&&b")]
    [InlineData("")]
    [InlineData("-flag")]
    [InlineData("a\nb")]
    [InlineData("a'b")]
    [InlineData("a\"b")]
    [InlineData("a>b")]
    public void ValidateRejectsInjection(string bad) =>
        Assert.Throws<DockerInvalidReferenceException>(() => DockerService.Validate(bad));

    [Fact]
    public void CommandBuildersMatchReferenceImplementation()
    {
        Assert.Equal("docker start plex", DockerService.StartCommand("plex"));
        Assert.Equal("docker stop plex", DockerService.StopCommand("plex"));
        Assert.Equal("docker restart plex", DockerService.RestartCommand("plex"));
        Assert.Equal("docker logs --tail 100 plex 2>&1", DockerService.LogsCommand("plex", 100));
        Assert.Equal("docker logs --tail 1 plex 2>&1", DockerService.LogsCommand("plex", 0));
        Assert.Equal(
            "docker ps -a --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null",
            DockerService.ListCommand(all: true));
        Assert.Equal(
            "docker ps --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null",
            DockerService.ListCommand(all: false));
    }

    [Fact]
    public void InjectionCannotReachCommandBuilder() =>
        Assert.Throws<DockerInvalidReferenceException>(() => DockerService.StopCommand("plex; rm -rf /"));

    [Fact]
    public void ParseListRunningAndStopped()
    {
        const string output = "a1b2c3|plex|linuxserver/plex:latest|Up 3 days\n" +
                               "d4e5f6|old|busybox|Exited (0) 2 hours ago";
        var list = DockerService.ParseList(output);

        Assert.Equal(2, list.Count);
        Assert.Equal("plex", list[0].Name);
        Assert.True(list[0].IsRunning);
        Assert.Equal("old", list[1].Name);
        Assert.False(list[1].IsRunning);
    }

    [Fact]
    public void ExecShellCommandFallsBackToSh() =>
        Assert.Equal(
            "docker exec -it plex sh -c 'command -v bash >/dev/null && exec bash || exec sh'",
            DockerService.ExecShellCommand("plex"));
}
