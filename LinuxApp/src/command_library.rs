//! Port av Sources/SSHCore/CommandLibrary.swift — statiskt, inbyggt
//! referenskommandobibliotek (VISION.md: "Docker, Linux, Git, Cloudflare,
//! Tailscale, WireGuard, systemd — varje kommando med beskrivning, exempel,
//! dokumentation"). Ren referensdata, ingen persistens (till skillnad från
//! `Snippet`).
//!
//! # Vad "varje kommando med beskrivning, exempel, dokumentation" betyder
//!
//! Beskrivning och dokumentationslänk gäller undantagslöst — varje post
//! har båda, och [`tests::every_entry_carries_summary_and_docs`] ser till
//! att det förblir så.
//!
//! Exempel gäller de kommandon som har `{{variabler}}`. Ett exempel finns
//! till för att visa hur en variabel fylls i; för `df -h` skulle
//! "exemplet" bli `df -h` igen, alltså brus i vyn snarare än hjälp. Den
//! avgränsningen är ett val, inte en lucka, och testas som en regel så
//! att en ny mall-post inte kan smyga in utan exempel.
//!
//! Läget innan 2026-08-18: 6 av 30 poster hade dokumentationslänk, och
//! INGEN hade både exempel och länk — `entry!`-makrot saknade en variant
//! som tog båda, så kravet gick inte att uttrycka.

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
    // Varianten som saknades. Utan den gick det inte att ge en post både
    // exempel och dokumentation — vilket är precis vad VISION begär, och
    // förklarar varför ingen av de 30 posterna hade båda.
    ($cat:expr, $cmd:expr, $summary:expr, example: $example:expr, docs_url: $docs:expr) => {
        Entry { category: $cat, command: $cmd, summary: $summary, example: Some($example), docs_url: Some($docs) }
    };
}

pub fn all() -> Vec<Entry> {
    use Category::*;
    vec![
        entry!(Docker, "docker ps -a", "Lista alla containrar (även stoppade)", docs_url: "https://docs.docker.com/reference/cli/docker/container/ls/"),
        entry!(Docker, "docker compose restart {{service}}", "Starta om en tjänst i Compose-projektet", example: "docker compose restart web", docs_url: "https://docs.docker.com/reference/cli/docker/compose/restart/"),
        entry!(Docker, "docker compose logs -f {{service}}", "Följ loggarna för en tjänst", example: "docker compose logs -f web", docs_url: "https://docs.docker.com/reference/cli/docker/compose/logs/"),
        entry!(Docker, "docker compose pull && docker compose up -d", "Hämta senaste images och uppdatera", docs_url: "https://docs.docker.com/reference/cli/docker/compose/up/"),
        entry!(Docker, "docker system df", "Diskanvändning per images/containrar/volymer", docs_url: "https://docs.docker.com/reference/cli/docker/system/df/"),
        entry!(Docker, "docker system prune -af", "Städa bort oanvända images/containrar/nätverk (försiktigt — permanent)", docs_url: "https://docs.docker.com/reference/cli/docker/system/prune/"),
        entry!(Docker, "docker exec -it {{container}} sh", "Öppna en shell i en container", example: "docker exec -it web sh", docs_url: "https://docs.docker.com/reference/cli/docker/container/exec/"),
        entry!(Linux, "df -h", "Diskutrymme per filsystem, läsbart", docs_url: "https://man7.org/linux/man-pages/man1/df.1.html"),
        entry!(Linux, "du -sh {{path}}/* | sort -rh | head -20", "20 största mapparna/filerna i en katalog", example: "du -sh /var/log/* | sort -rh | head -20", docs_url: "https://man7.org/linux/man-pages/man1/du.1.html"),
        entry!(Linux, "journalctl -u {{service}} -f", "Följ loggarna för en systemd-tjänst", example: "journalctl -u nginx -f", docs_url: "https://www.freedesktop.org/software/systemd/man/latest/journalctl.html"),
        entry!(Linux, "ss -tlnp", "Lyssnande TCP-portar + vilken process som äger dem", docs_url: "https://man7.org/linux/man-pages/man8/ss.8.html"),
        entry!(Linux, "uname -a", "Kernel- och OS-version", docs_url: "https://man7.org/linux/man-pages/man1/uname.1.html"),
        entry!(Linux, "free -h", "Minnesanvändning, läsbart", docs_url: "https://man7.org/linux/man-pages/man1/free.1.html"),
        entry!(Git, "git log --oneline -{{n}}", "De {{n}} senaste committen, en rad var", example: "git log --oneline -20", docs_url: "https://git-scm.com/docs/git-log"),
        entry!(Git, "git fetch --all --prune", "Hämta alla remotes, ta bort borttagna grenar lokalt", docs_url: "https://git-scm.com/docs/git-fetch"),
        entry!(Git, "git branch -vv", "Alla lokala grenar + vilken remote-gren de spårar", docs_url: "https://git-scm.com/docs/git-branch"),
        entry!(Git, "git diff --stat {{base}}..HEAD", "Vilka filer ändrats sedan en viss punkt", example: "git diff --stat main..HEAD", docs_url: "https://git-scm.com/docs/git-diff"),
        entry!(Cloudflare, "cloudflared tunnel list", "Lista aktiva Cloudflare-tunnlar på den här maskinen", docs_url: "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/"),
        entry!(Cloudflare, "cloudflared tunnel info {{tunnel}}", "Detaljer om en specifik tunnel", example: "cloudflared tunnel info mp100", docs_url: "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/do-more-with-tunnels/"),
        entry!(Cloudflare, "systemctl status cloudflared", "Status för cloudflared-tjänsten", docs_url: "https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/configure-tunnels/local-management/as-a-service/linux/"),
        entry!(Tailscale, "tailscale status", "Anslutna noder i Tailscale-nätverket + status", docs_url: "https://tailscale.com/kb/1080/cli"),
        entry!(Tailscale, "tailscale ping {{host}}", "Ping över Tailscale-nätverket (visar vilken väg paketet tog)", example: "tailscale ping mp100", docs_url: "https://tailscale.com/kb/1080/cli"),
        entry!(Tailscale, "tailscale ip -4", "Den här enhetens Tailscale-IP", docs_url: "https://tailscale.com/kb/1080/cli"),
        entry!(WireGuard, "wg show", "Aktiva WireGuard-interface, peers och senaste handskakning", docs_url: "https://www.wireguard.com/quickstart/"),
        entry!(WireGuard, "wg-quick up {{interface}}", "Starta ett WireGuard-interface från dess konfigfil", example: "wg-quick up wg0", docs_url: "https://man7.org/linux/man-pages/man8/wg-quick.8.html"),
        entry!(WireGuard, "wg-quick down {{interface}}", "Stäng ett WireGuard-interface", example: "wg-quick down wg0", docs_url: "https://man7.org/linux/man-pages/man8/wg-quick.8.html"),
        entry!(Systemd, "systemctl status {{service}}", "Status för en tjänst", example: "systemctl status docker", docs_url: "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html"),
        entry!(Systemd, "systemctl restart {{service}}", "Starta om en tjänst", example: "systemctl restart nginx", docs_url: "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html"),
        entry!(Systemd, "systemctl list-units --failed", "Alla tjänster som för närvarande felar", docs_url: "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html"),
        entry!(Systemd, "systemctl enable --now {{service}}", "Aktivera en tjänst vid uppstart och starta den nu", example: "systemctl enable --now docker", docs_url: "https://www.freedesktop.org/software/systemd/man/latest/systemctl.html"),
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

    /// VISION-kravet som regel i stället för som ambition. Sex av trettio
    /// poster hade dokumentationslänk innan den här kördes första gången.
    #[test]
    fn every_entry_carries_summary_and_docs() {
        for e in all() {
            assert!(!e.summary.trim().is_empty(), "{} saknar beskrivning", e.command);
            let url = e.docs_url.unwrap_or_else(|| panic!("{} saknar dokumentationslänk", e.command));
            assert!(
                url.starts_with("https://"),
                "{}: dokumentationslänken ska vara https, är {url}",
                e.command
            );
        }
    }

    /// Ett exempel finns för att visa hur en variabel fylls i. Har
    /// kommandot inga variabler blir exemplet en upprepning av kommandot
    /// — därför gäller kravet mallarna, och bara dem.
    #[test]
    fn templated_commands_carry_an_example_that_fills_the_variables_in() {
        for e in all() {
            if !e.command.contains("{{") {
                continue;
            }
            let example = e
                .example
                .unwrap_or_else(|| panic!("{} är en mall men saknar exempel", e.command));
            assert!(
                !example.contains("{{"),
                "{}: exemplet ska visa ifyllda värden, inte mallen igen ({example})",
                e.command
            );
        }
    }

    #[test]
    fn category_labels_match_swift_raw_values() {
        assert_eq!(Category::Docker.label(), "Docker");
        assert_eq!(Category::WireGuard.label(), "WireGuard");
        assert_eq!(Category::Systemd.label(), "systemd");
    }
}
