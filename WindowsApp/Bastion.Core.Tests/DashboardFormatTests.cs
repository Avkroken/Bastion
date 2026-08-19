using System.Globalization;
using Bastion.Core;

namespace Bastion.Core.Tests;

public class DashboardFormatTests
{
    [Theory]
    [InlineData(0, "0,0 B")]
    [InlineData(1536, "1,5 KiB")]
    [InlineData(15244848L * 1024, "14,5 GiB")]
    [InlineData(-1, "0,0 B")]
    public void BytesUseBinaryPrefixes(long bytes, string expected)
    {
        var previous = Thread.CurrentThread.CurrentCulture;
        Thread.CurrentThread.CurrentCulture = new CultureInfo("sv-SE");
        try
        {
            Assert.Equal(expected, DashboardFormat.Bytes(bytes));
        }
        finally
        {
            Thread.CurrentThread.CurrentCulture = previous;
        }
    }

    [Fact]
    public void PercentRoundsAndClamps()
    {
        Assert.Equal(29, DashboardFormat.Percent(0.2857));
        Assert.Equal(100, DashboardFormat.Percent(1.4));
        Assert.Equal(0, DashboardFormat.Percent(-0.2));
        Assert.Equal(50, DashboardFormat.Percent(0.495));
    }

    [Theory]
    [InlineData(259335.25, "3d 0h 2m")]
    [InlineData(3720, "1h 2m")]
    [InlineData(120, "2m")]
    [InlineData(-5, "0m")]
    public void UptimeDropsUnitsItDoesNotNeed(double seconds, string expected) =>
        Assert.Equal(expected, DashboardFormat.Uptime(seconds));

    [Fact]
    public void LevelFollowsTheThresholdsTheUiPaintsWith()
    {
        Assert.Equal(MetricLevel.Ok, DashboardFormat.Level(69));
        Assert.Equal(MetricLevel.Warning, DashboardFormat.Level(70));
        Assert.Equal(MetricLevel.Warning, DashboardFormat.Level(87));
        Assert.Equal(MetricLevel.Critical, DashboardFormat.Level(88));
    }

    [Fact]
    public void MemoryFractionSurvivesTheWholeWay()
    {
        var snapshot = SystemProbe.Parse("@@MEM\nMemTotal:  1000000 kB\nMemAvailable:  250000 kB\n@@END");

        Assert.Equal(75, DashboardFormat.Percent(snapshot.Memory!.UsedFraction));
    }
}
