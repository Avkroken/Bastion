//! Port av Sources/SSHCore/CommandLibrary.swift — statiskt, inbyggt
//! referenskommandobibliotek (VISION.md: "Docker, Linux, Git, Cloudflare,
//! Tailscale, WireGuard, systemd — varje kommando med beskrivning, exempel,
//! dokumentation"). Ren referensdata, ingen persistens (till skillnad från
//! `Snippet`).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Docker,
    Linux,
    Git,
    Cloudflare,
    Tailscale,
    WireGuard,
    Systemd,
}

impl Category {
    pub fn label(&self) -> &'static str {
        match self {
            Category::Docker => "Docker",
            Category::Linux => "Linux",
            Category::Git => "Git",
            Category::Cloudflare => "Cloudflare",
            Category::Tailscale => "Tailscale",
            Category::WireGuard => "WireGuard",
            Category::Systemd => "systemd",
        }
    }
}

pub struct Entry {
    pub category: Category,
    pub command: &'static str,
    pub summary: &'static str,
    pub example: Option<&'static str>,
    pub docs_url: Option<&'static str>,
}

macro_rules! entry {
    ($cat:expr, $cmd:expr, $summary:expr) => {
        Entry { category: $cat, command: $cmd, summary: $summary, example: None, docs_url: None }
    };
    ($cat:expr, $cmd:expr, $summary:expr, example: $example:expr) => {
        Entry { category: $cat, command: $cmd, summary: $summary, example: Some($example), docs_url: None }
    };
    ($cat:expr, $cmd:expr, $summary:expr, docs_url: $docs:expr) => {
        Entry { category: $cat, command: $cmd, summary: $summary, example: None, docs_url: Some($docs) }
    };
}

pub fn all() -> Vec<Entry> {
    use Category::*;
    vec![
        entry!(Docker, "docker ps -a", "Lista alla containrar (även stoppade)"),
        entry!(Docker, "docker compose restart {{service}}", "Starta om en tjänst i Compose-projektet", example: "docker compose restart web"),
        entry!(Docker, "docker compose logs -f {{service}}", "Följ loggarna för en tjänst", example: "docker compose logs -f web"),
        entry!(Docker, "docker compose pull && docker compose up -d", "Hämta senaste images och uppdatera"),
        entry!(Docker, "docker system df", "Diskanvändning per images/containrar/volymer"),
        entry!(Docker, "docker system prune -af", "Städa bort oanvända images/containrar/nätverk (försiktigt — permanent)"),
        entry!(Docker, "docker exec -it {{container}} sh", "Öppna en shell i en container", example: "docker exec -it web sh"),
        entry!(Linux, "df -h", "Diskutrymme per filsystem, läsbart"),
        entry!(Linux, "du -sh {{path}}/* | sort -rh | head -20", "20 största mapparna/filerna i en katalog", example: "du -sh /var/log/* | sort -rh | head -20"),
        entry!(Linux, "journalctl -u {{service}} -f", "Följ loggarna för en systemd-tjänst", example: "journalctl -u nginx -f"),
        entry!(Linux, "ss -tlnp", "Lyssnande TCP-portar + vilken process som äger dem"),
        entry!(Linux, "uname -a", "Kernel- och OS-version"),
        entry!(Linux, "free -h", "Minnesanvändning, läsbart"),
        entry!(Git, "git log --oneline -{{n}}", "De {{n}} senaste committen, en rad var", example: "git log --oneline -20"),
        entry!(Git, "git fetch --all --prune", "Hämta alla remotes, ta bort borttagna grenar lokalt"),
        entry!(Git, "git branch -vv", "Alla lokala grenar + vilken remote-gren de spårar"),
        entry!(Git, "git diff --stat {{base}}..HEAD", "Vilka filer ändrats sedan en viss punkt", example: "git diff --stat main..HEAD"),
        entry!(Cloudflare, "cloudflared tunnel list", "Lista aktiva Cloudflare-tunnlar på den här maskinen", docs_url: "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/"),
        entry!(Cloudflare, "cloudflared tunnel info {{tunnel}}", "Detaljer om en specifik tunnel", example: "cloudflared tunnel info mp100"),
        entry!(Cloudflare, "systemctl status cloudflared", "Status för cloudflared-tjänsten"),
        entry!(Tailscale, "tailscale status", "Anslutna noder i Tailscale-nätverket + status", docs_url: "https://tailscale.com/kb/1080/cli"),
        entry!(Tailscale, "tailscale ping {{host}}", "Ping över Tailscale-nätverket (visar vilken väg paketet tog)", example: "tailscale ping mp100"),
        entry!(Tailscale, "tailscale ip -4", "Den här enhetens Tailscale-IP"),
        entry!(WireGuard, "wg show", "Aktiva WireGuard-interface, peers och senaste handskakning", docs_url: "https://www.wireguard.com/quickstart/"),
        entry!(WireGuard, "wg-quick up {{interface}}", "Starta ett WireGuard-interface från dess konfigfil", example: "wg-quick up wg0"),
        entry!(WireGuard, "wg-quick down {{interface}}", "Stäng ett WireGuard-interface", example: "wg-quick down wg0"),
        entry!(Systemd, "systemctl status {{service}}", "Status för en tjänst", example: "systemctl status docker"),
        entry!(Systemd, "systemctl restart {{service}}", "Starta om en tjänst", example: "systemctl restart nginx"),
        entry!(Systemd, "systemctl list-units --failed", "Alla tjänster som för närvarande felar"),
        entry!(Systemd, "systemctl enable --now {{service}}", "Aktivera en tjänst vid uppstart och starta den nu", example: "systemctl enable --now docker"),
    ]
}

/// Filtrerad vy, testad men inte konsumerad av UI:t än — nuvarande
/// Kommandobibliotek-vy listar allt platt. Kategorigrupperad UI är en
/// naturlig utbyggnad om listan känns för lång i praktiken.
#[allow(dead_code)]
pub fn entries_in(category: Category) -> Vec<Entry> {
    all().into_iter().filter(|e| e.category == category).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_entries_are_present_matching_swift_count() {
        // Sources/SSHCore/CommandLibrary.swift har 30 poster (7+6+4+3+3+3+4).
        assert_eq!(all().len(), 30);
    }

    #[test]
    fn entries_in_filters_by_category() {
        let docker = entries_in(Category::Docker);
        assert_eq!(docker.len(), 7);
        assert!(docker.iter().all(|e| e.category == Category::Docker));
    }

    #[test]
    fn category_labels_match_swift_raw_values() {
        assert_eq!(Category::Docker.label(), "Docker");
        assert_eq!(Category::WireGuard.label(), "WireGuard");
        assert_eq!(Category::Systemd.label(), "systemd");
    }
}
