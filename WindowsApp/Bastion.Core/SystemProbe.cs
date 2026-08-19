namespace Bastion.Core;

/// <summary>Port av <c>LoadAverage</c> i Sources/SSHCore/SystemProbe.swift.</summary>
public sealed record LoadAverage(double One, double Five, double Fifteen);

/// <summary>Port av <c>MemoryInfo</c> — <see cref="UsedBytes"/> härleds, som på Swift-sidan.</summary>
public sealed record MemoryInfo(long TotalBytes, long AvailableBytes)
{
    public long UsedBytes => Math.Max(0, TotalBytes - AvailableBytes);

    public double UsedFraction => TotalBytes > 0 ? (double)UsedBytes / TotalBytes : 0;
}

/// <summary>Port av <c>DiskUsage</c>.</summary>
public sealed record DiskUsage(
    string Filesystem,
    string Mount,
    long SizeBytes,
    long UsedBytes,
    long AvailableBytes,
    int CapacityPercent);

/// <summary>En temperaturgivare ur <c>/sys/class/thermal</c>. Sysfs och inte
/// <c>sensors</c>: lm-sensors är sällan installerat på en server, och VISION
/// säger uttryckligen "allt via SSH, ingen agent krävs".</summary>
public sealed record Temperature(string Label, double Celsius);

/// <summary>En IP-adress på ett gränssnitt. <see cref="Address"/> bär prefixlängden
/// precis som <c>ip</c> skriver den: <c>192.168.1.10/24</c>.</summary>
public sealed record IpAddress(string Interface, string Address, bool IsIpv6);

/// <summary>En nyckel i den inloggade användarens <c>authorized_keys</c> — alltså
/// vem som KAN logga in på kontot, inte värdens egna värdnycklar.</summary>
public sealed record AuthorizedKey(int Bits, string Fingerprint, string Comment, string Algorithm);

/// <summary>En inloggad användare ur <c>who</c>. <see cref="From"/> är null för en
/// lokal inloggning — skillnaden mot "loggade in från 192.168.1.5" är värd att behålla.</summary>
public sealed record ActiveUser(string User, string Tty, string Since, string? From);

/// <summary>Port av <c>SystemSnapshot</c>. Allt utom listorna är valfritt — en
/// värd som saknar <c>docker</c> eller <c>nproc</c> ger bara färre fält, inget fel.</summary>
public sealed record SystemSnapshot
{
    public string? Hostname { get; init; }
    public string? Os { get; init; }
    public string? Kernel { get; init; }
    public int? CpuCount { get; init; }
    public double? UptimeSeconds { get; init; }
    public LoadAverage? Load { get; init; }
    public MemoryInfo? Memory { get; init; }
    public IReadOnlyList<DiskUsage> Disks { get; init; } = [];
    public IReadOnlyList<DockerContainer> Containers { get; init; } = [];
    public IReadOnlyList<Temperature> Temperatures { get; init; } = [];
    public IReadOnlyList<IpAddress> Addresses { get; init; } = [];
    public IReadOnlyList<AuthorizedKey> AuthorizedKeys { get; init; } = [];
    public IReadOnlyList<ActiveUser> ActiveUsers { get; init; } = [];

    /// <summary>Rot-filsystemet, om det finns — det UI:t visar först.</summary>
    public DiskUsage? RootDisk => Disks.FirstOrDefault(d => d.Mount == "/");
}

/// <summary>
/// Port av Sources/SSHCore/SystemProbe.swift (samma design som
/// LinuxApp/src/dashboard.rs): dashboard-data hämtad agentlöst över SSH.
/// Ett kombinerat kommando ger en ögonblicksbild; parsningen är rena
/// funktioner (sträng → record) och testas mot fixtures, SSH-lagret är tunt
/// ovanpå. Allt proben kör är läsning.
/// </summary>
public static class SystemProbe
{
    /// <summary>Ett kommando, en round-trip. Sektionsmarkörer (@@NAMN) skiljer utdata åt.</summary>
    public static string Command { get; } = string.Join("; ",
    [
        "echo @@LOADAVG", "cat /proc/loadavg 2>/dev/null",
        "echo @@UPTIME", "cat /proc/uptime 2>/dev/null",
        "echo @@MEM", "cat /proc/meminfo 2>/dev/null",
        "echo @@DF", "df -kP 2>/dev/null",
        "echo @@OS", "cat /etc/os-release 2>/dev/null",
        "echo @@KERNEL", "uname -sr 2>/dev/null",
        "echo @@HOST", "cat /proc/sys/kernel/hostname 2>/dev/null",
        "echo @@NPROC", "nproc 2>/dev/null",
        "echo @@DOCKER", "docker ps --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null",
        "echo @@TEMP",
        "for z in /sys/class/thermal/thermal_zone*; do "
            + "[ -r \"$z/temp\" ] && echo \"$(cat \"$z/type\" 2>/dev/null)|$(cat \"$z/temp\" 2>/dev/null)\"; done 2>/dev/null",
        "echo @@IP", "ip -o addr show scope global 2>/dev/null",
        "echo @@IPFALLBACK", "hostname -I 2>/dev/null",
        "echo @@KEYS", "ssh-keygen -l -f \"$HOME/.ssh/authorized_keys\" 2>/dev/null",
        "echo @@WHO", "who 2>/dev/null",
        "echo @@END",
    ]);

    /// <summary>Kör proben över en ansluten värd.</summary>
    public static SystemSnapshot Snapshot(Host host, string? password, KnownHosts knownHosts) =>
        Parse(SshSession.RunCommand(host, password, knownHosts, Command));

    public static SystemSnapshot Parse(string output)
    {
        var sections = new Dictionary<string, List<string>>(StringComparer.Ordinal);
        var current = "";
        foreach (var line in output.Split(['\n', '\r'], StringSplitOptions.RemoveEmptyEntries))
        {
            if (line.StartsWith("@@", StringComparison.Ordinal))
            {
                current = line[2..];
                if (!sections.ContainsKey(current)) sections[current] = [];
            }
            else if (current.Length > 0)
            {
                sections[current].Add(line);
            }
        }

        return new SystemSnapshot
        {
            Load = ParseLoad(First(sections, "LOADAVG")),
            UptimeSeconds = ParseDouble(First(sections, "UPTIME")?.Split(' ').FirstOrDefault()),
            Memory = ParseMemory(Lines(sections, "MEM")),
            Disks = ParseDisks(Lines(sections, "DF")),
            Os = ParseOs(Lines(sections, "OS")),
            Kernel = NonEmpty(First(sections, "KERNEL")?.Trim()),
            Hostname = NonEmpty(First(sections, "HOST")?.Trim()),
            CpuCount = ParseInt(First(sections, "NPROC")?.Trim()),
            Containers = DockerService.ParseList(string.Join("\n", Lines(sections, "DOCKER"))),
            Temperatures = ParseTemperatures(Lines(sections, "TEMP")),
            // `ip` saknas på minimala värdar; `hostname -I` är reserven, och
            // först när den primära vägen inte gav något — annars såg en tom
            // `ip`-utdata ut som "värden har inga adresser".
            Addresses = ParseAddresses(Lines(sections, "IP")) is { Count: > 0 } addresses
                ? addresses
                : ParseFallbackAddresses(Lines(sections, "IPFALLBACK")),
            AuthorizedKeys = ParseAuthorizedKeys(Lines(sections, "KEYS")),
            ActiveUsers = ParseActiveUsers(Lines(sections, "WHO")),
        };
    }

    private static List<string> Lines(Dictionary<string, List<string>> sections, string key) =>
        sections.TryGetValue(key, out var v) ? v : [];

    private static string? First(Dictionary<string, List<string>> sections, string key) =>
        Lines(sections, key).FirstOrDefault();

    private static string? NonEmpty(string? value) => string.IsNullOrEmpty(value) ? null : value;

    /// <summary>Punkt som decimaltecken oavsett värdens/klientens språkinställning.</summary>
    private static double? ParseDouble(string? text) =>
        double.TryParse(text, System.Globalization.NumberStyles.Float,
            System.Globalization.CultureInfo.InvariantCulture, out var v) ? v : null;

    private static int? ParseInt(string? text) => int.TryParse(text, out var v) ? v : null;

    private static long? ParseLong(string? text) => long.TryParse(text, out var v) ? v : null;

    private static LoadAverage? ParseLoad(string? line)
    {
        var parts = (line ?? "").Split(' ', StringSplitOptions.RemoveEmptyEntries)
            .Select(ParseDouble).Where(v => v is not null).Select(v => v!.Value).ToList();
        return parts.Count >= 3 ? new LoadAverage(parts[0], parts[1], parts[2]) : null;
    }

    private static MemoryInfo? ParseMemory(List<string> lines)
    {
        var kb = new Dictionary<string, long>(StringComparer.Ordinal);
        foreach (var line in lines)
        {
            var f = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (f.Length < 2) continue;
            var key = f[0].EndsWith(':') ? f[0][..^1] : f[0];
            if (ParseLong(f[1]) is { } value) kb[key] = value;
        }
        if (!kb.TryGetValue("MemTotal", out var total) || !kb.TryGetValue("MemAvailable", out var avail))
            return null;
        return new MemoryInfo(total * 1024, avail * 1024);
    }

    private static List<DiskUsage> ParseDisks(List<string> lines)
    {
        var disks = new List<DiskUsage>();
        foreach (var line in lines)
        {
            var f = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);
            if (f.Length < 6 || f[0] == "Filesystem") continue;
            if (ParseLong(f[1]) is not { } blocks) continue;
            if (ParseLong(f[2]) is not { } used) continue;
            if (ParseLong(f[3]) is not { } avail) continue;
            var capacity = ParseInt(f[4].Replace("%", "")) ?? 0;
            disks.Add(new DiskUsage(
                f[0], string.Join(" ", f[5..]),
                blocks * 1024, used * 1024, avail * 1024, capacity));
        }
        return disks;
    }

    /// <summary>
    /// <c>type|temp</c> per zon, där temperaturen är MILLIGRADER — ett rått
    /// <c>55000</c> i gränssnittet ser ut som en trasig sensor, inte som 55 °C.
    /// Orimliga värden filtreras bort: zoner utan riktig givare rapporterar
    /// <c>-274000</c> eller nollor, och under absoluta nollpunkten är inte data.
    /// </summary>
    private static List<Temperature> ParseTemperatures(List<string> lines)
    {
        var out_ = new List<Temperature>();
        foreach (var line in lines)
        {
            var split = line.Split('|', 2);
            if (split.Length < 2) continue;
            if (ParseDouble(split[1].Trim()) is not { } milli) continue;
            var celsius = milli / 1000.0;
            if (celsius is < -50 or > 200) continue;
            var label = split[0].Trim();
            out_.Add(new Temperature(label.Length == 0 ? "okänd" : label, celsius));
        }
        return out_;
    }

    /// <summary>
    /// <c>ip -o addr show scope global</c> ger rader som
    /// <c>2: eth0    inet 192.168.1.10/24 brd … scope global eth0</c>.
    /// <c>scope global</c> i kommandot gör att loopback och link-local aldrig
    /// når hit — de säger ingenting om hur maskinen nås utifrån.
    /// </summary>
    private static List<IpAddress> ParseAddresses(List<string> lines)
    {
        var out_ = new List<IpAddress>();
        foreach (var line in lines)
        {
            var f = line.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
            // index 0 är "2:", 1 är gränssnittet, 2 är inet/inet6, 3 adressen
            if (f.Length < 4) continue;
            var isIpv6 = f[2] switch { "inet" => false, "inet6" => true, _ => (bool?)null };
            if (isIpv6 is null) continue;
            out_.Add(new IpAddress(f[1], f[3], isIpv6.Value));
        }
        return out_;
    }

    /// <summary><c>hostname -I</c> ger adresserna mellanslagsseparerade och UTAN
    /// gränssnitt — priset för att fungera på en värd utan iproute2. Gränssnittet
    /// blir "okänt" snarare än påhittat.</summary>
    private static List<IpAddress> ParseFallbackAddresses(List<string> lines) =>
        lines
            .SelectMany(l => l.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries))
            // Ingen prefixlängd att gå på, så typen avgörs av kolon: en IPv6-adress
            // innehåller alltid minst ett, en IPv4 aldrig.
            .Select(a => new IpAddress("okänt", a, a.Contains(':')))
            .ToList();

    /// <summary>
    /// <c>ssh-keygen -l -f authorized_keys</c> ger
    /// <c>256 SHA256:abc… anders@laptop (ED25519)</c>. Kommentaren kan innehålla
    /// mellanslag och algoritmen står inom parentes SIST — därför plockas båda
    /// ändarna först och kommentaren blir det som blir kvar i mitten.
    /// </summary>
    private static List<AuthorizedKey> ParseAuthorizedKeys(List<string> lines)
    {
        var out_ = new List<AuthorizedKey>();
        foreach (var raw in lines)
        {
            var line = raw.Trim();
            var open = line.LastIndexOf('(');
            if (open < 0 || !line.EndsWith(')')) continue;
            var algorithm = line[(open + 1)..^1];
            var rest = line[..open].Trim();
            var parts = rest.Split((char[]?)null, 3, StringSplitOptions.RemoveEmptyEntries);
            if (parts.Length < 2 || ParseInt(parts[0]) is not { } bits) continue;
            var comment = parts.Length > 2 ? parts[2].Trim() : "";
            out_.Add(new AuthorizedKey(bits, parts[1], comment, algorithm));
        }
        return out_;
    }

    /// <summary>
    /// <c>who</c> ger <c>anders   pts/0        2026-08-18 19:14 (192.168.1.5)</c>.
    /// Ursprunget står inom parentes och saknas för lokala inloggningar — därför
    /// null, inte tom sträng.
    /// </summary>
    private static List<ActiveUser> ParseActiveUsers(List<string> lines)
    {
        var out_ = new List<ActiveUser>();
        foreach (var raw in lines)
        {
            var line = raw.Trim();
            string? from = null;
            var rest = line;
            var open = line.LastIndexOf('(');
            if (open >= 0 && line.EndsWith(')'))
            {
                from = line[(open + 1)..^1];
                rest = line[..open].Trim();
            }
            var f = rest.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);
            if (f.Length < 3) continue;
            out_.Add(new ActiveUser(f[0], f[1], string.Join(" ", f[2..]), from));
        }
        return out_;
    }

    private static string? ParseOs(List<string> lines)
    {
        const string prefix = "PRETTY_NAME=";
        foreach (var line in lines)
        {
            if (!line.StartsWith(prefix, StringComparison.Ordinal)) continue;
            var value = line[prefix.Length..];
            if (value.Length >= 2 && value.StartsWith('"') && value.EndsWith('"'))
                value = value[1..^1];
            return NonEmpty(value);
        }
        return null;
    }
}
