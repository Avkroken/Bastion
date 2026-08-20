using System.Globalization;

namespace Bastion.Core;

/// <summary>
/// Textformatering för dashboarden. Ligger i kärnan och inte i XAML-koden
/// eftersom det är just den sortens logik som tyst blir fel (milligrader,
/// binära prefix, negativ drifttid) — här går den att testa. Samma format som
/// <c>format_bytes</c>/<c>format_uptime</c> i LinuxApp/src/main.rs.
/// </summary>
public static class DashboardFormat
{
    private static readonly string[] Units = ["B", "KiB", "MiB", "GiB", "TiB"];

    /// <summary>Binära prefix med en decimal: <c>1536</c> → <c>1,5 KiB</c>.</summary>
    public static string Bytes(long bytes)
    {
        double value = Math.Max(0, bytes);
        var unit = 0;
        while (value >= 1024 && unit < Units.Length - 1)
        {
            value /= 1024;
            unit++;
        }
        return $"{value.ToString("0.0", CultureInfo.CurrentCulture)} {Units[unit]}";
    }

    /// <summary>Andel 0–1 som heltalsprocent, klippt till intervallet.</summary>
    public static int Percent(double fraction) =>
        (int)Math.Round(Math.Clamp(fraction, 0, 1) * 100, MidpointRounding.AwayFromZero);

    /// <summary>Drifttid: <c>2d 3h 4m</c>, <c>3h 4m</c> eller <c>4m</c>.</summary>
    public static string Uptime(double seconds)
    {
        var total = (long)Math.Max(0, seconds);
        var days = total / 86400;
        var hours = total % 86400 / 3600;
        var minutes = total % 3600 / 60;
        if (days > 0) return $"{days}d {hours}h {minutes}m";
        if (hours > 0) return $"{hours}h {minutes}m";
        return $"{minutes}m";
    }

    /// <summary>Belastningen som den skrivs i UI:t: <c>2,53 · 1,86 · 1,81</c>.</summary>
    public static string Load(LoadAverage load) =>
        string.Join(" · ", new[] { load.One, load.Five, load.Fifteen }
            .Select(v => v.ToString("0.00", CultureInfo.CurrentCulture)));

    /// <summary>
    /// Hur allvarligt ett mätvärde är — samma trösklar som prototypen ritar med
    /// (grönt under 70 %, gult under 88 %, rött däröver). UI:t väljer färg på den
    /// här, så gränserna står på ett ställe.
    /// </summary>
    public static MetricLevel Level(int percent) =>
        percent >= 88 ? MetricLevel.Critical : percent >= 70 ? MetricLevel.Warning : MetricLevel.Ok;
}

public enum MetricLevel
{
    Ok,
    Warning,
    Critical,
}
