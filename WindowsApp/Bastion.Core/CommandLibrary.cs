namespace Bastion.Core;

/// <summary>
/// Port av Sources/SSHCore/CommandLibrary.swift. Statiskt, inbyggt
/// referensbibliotek (VISION.md: "Docker, Linux, Git, Cloudflare, Tailscale,
/// WireGuard, systemd — varje kommando med beskrivning, exempel, dokumentation").
/// Ren referensdata — ingen persistens, till skillnad från <see cref="Snippet"/>.
/// </summary>
public enum CommandLibraryCategory
{
    Docker, Linux, Git, Cloudflare, Tailscale, WireGuard, Systemd,
}

public sealed record CommandLibraryEntry(
    CommandLibraryCategory Category, string Command, string Summary, string? Example = null, string? DocsUrl = null)
{
    public string Id => $"{Category}/{Command}";

    /// <summary>Som ett Snippet — återanvänder samma {{variabel}}-rendering utan att duplicera logiken.</summary>
    public Snippet AsSnippet => Snippet.Create(Summary, Command);
}

public static class CommandLibrary
{
    public static readonly IReadOnlyList<CommandLibraryEntry> All = new List<CommandLibraryEntry>
    {
        // Docker
        new(CommandLibraryCategory.Docker, "docker ps -a", "Lista alla containrar (även stoppade)"),
        new(CommandLibraryCategory.Docker, "docker compose restart {{service}}", "Starta om en tjänst i Compose-projektet",
            "docker compose restart web"),
        new(CommandLibraryCategory.Docker, "docker compose logs -f {{service}}", "Följ loggarna för en tjänst",
            "docker compose logs -f web"),
        new(CommandLibraryCategory.Docker, "docker compose pull && docker compose up -d", "Hämta senaste images och uppdatera"),
        new(CommandLibraryCategory.Docker, "docker system df", "Diskanvändning per images/containrar/volymer"),
        new(CommandLibraryCategory.Docker, "docker system prune -af", "Städa bort oanvända images/containrar/nätverk (försiktigt — permanent)"),
        new(CommandLibraryCategory.Docker, "docker exec -it {{container}} sh", "Öppna en shell i en container",
            "docker exec -it web sh"),

        // Linux
        new(CommandLibraryCategory.Linux, "df -h", "Diskutrymme per filsystem, läsbart"),
        new(CommandLibraryCategory.Linux, "du -sh {{path}}/* | sort -rh | head -20", "20 största mapparna/filerna i en katalog",
            "du -sh /var/log/* | sort -rh | head -20"),
        new(CommandLibraryCategory.Linux, "journalctl -u {{service}} -f", "Följ loggarna för en systemd-tjänst",
            "journalctl -u nginx -f"),
        new(CommandLibraryCategory.Linux, "ss -tlnp", "Lyssnande TCP-portar + vilken process som äger dem"),
        new(CommandLibraryCategory.Linux, "uname -a", "Kernel- och OS-version"),
        new(CommandLibraryCategory.Linux, "free -h", "Minnesanvändning, läsbart"),

        // Git
        new(CommandLibraryCategory.Git, "git log --oneline -{{n}}", "De {{n}} senaste committen, en rad var",
            "git log --oneline -20"),
        new(CommandLibraryCategory.Git, "git fetch --all --prune", "Hämta alla remotes, ta bort borttagna grenar lokalt"),
        new(CommandLibraryCategory.Git, "git branch -vv", "Alla lokala grenar + vilken remote-gren de spårar"),
        new(CommandLibraryCategory.Git, "git diff --stat {{base}}..HEAD", "Vilka filer ändrats sedan en viss punkt",
            "git diff --stat main..HEAD"),

        // Cloudflare
        new(CommandLibraryCategory.Cloudflare, "cloudflared tunnel list", "Lista aktiva Cloudflare-tunnlar på den här maskinen",
            DocsUrl: "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/"),
        new(CommandLibraryCategory.Cloudflare, "cloudflared tunnel info {{tunnel}}", "Detaljer om en specifik tunnel",
            "cloudflared tunnel info mp100"),
        new(CommandLibraryCategory.Cloudflare, "systemctl status cloudflared", "Status för cloudflared-tjänsten"),

        // Tailscale
        new(CommandLibraryCategory.Tailscale, "tailscale status", "Anslutna noder i Tailscale-nätverket + status",
            DocsUrl: "https://tailscale.com/kb/1080/cli"),
        new(CommandLibraryCategory.Tailscale, "tailscale ping {{host}}", "Ping över Tailscale-nätverket (visar vilken väg paketet tog)",
            "tailscale ping mp100"),
        new(CommandLibraryCategory.Tailscale, "tailscale ip -4", "Den här enhetens Tailscale-IP"),

        // WireGuard
        new(CommandLibraryCategory.WireGuard, "wg show", "Aktiva WireGuard-interface, peers och senaste handskakning",
            DocsUrl: "https://www.wireguard.com/quickstart/"),
        new(CommandLibraryCategory.WireGuard, "wg-quick up {{interface}}", "Starta ett WireGuard-interface från dess konfigfil",
            "wg-quick up wg0"),
        new(CommandLibraryCategory.WireGuard, "wg-quick down {{interface}}", "Stäng ett WireGuard-interface",
            "wg-quick down wg0"),

        // systemd
        new(CommandLibraryCategory.Systemd, "systemctl status {{service}}", "Status för en tjänst",
            "systemctl status docker"),
        new(CommandLibraryCategory.Systemd, "systemctl restart {{service}}", "Starta om en tjänst",
            "systemctl restart nginx"),
        new(CommandLibraryCategory.Systemd, "systemctl list-units --failed", "Alla tjänster som för närvarande felar"),
        new(CommandLibraryCategory.Systemd, "systemctl enable --now {{service}}", "Aktivera en tjänst vid uppstart och starta den nu",
            "systemctl enable --now docker"),
    };

    public static IEnumerable<CommandLibraryEntry> Entries(CommandLibraryCategory category) =>
        All.Where(e => e.Category == category);
}
