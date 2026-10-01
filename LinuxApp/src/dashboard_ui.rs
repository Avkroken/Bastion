use adw::prelude::*;

use crate::dashboard;
use crate::humanize::{format_bytes, format_uptime};

/// Formaterar en `SystemSnapshot` som en rad `adw::ActionRow`-poster —
/// sammanfattning, drifttid, last, minne, en rad per disk, en rad per
/// Docker-container och de övriga observerade systemsektionerna.
pub(crate) fn build_dashboard_rows(
    snap: &dashboard::SystemSnapshot,
) -> Vec<adw::ActionRow> {
    let mut rows = Vec::new();

    let mut summary = Vec::new();
    if let Some(os) = &snap.os {
        summary.push(os.clone());
    }
    if let Some(kernel) = &snap.kernel {
        summary.push(kernel.clone());
    }
    if let Some(cpu) = snap.cpu_count {
        summary.push(format!("{cpu} kärnor"));
    }
    rows.push(
        adw::ActionRow::builder()
            .title(snap.hostname.clone().unwrap_or_else(|| "Värd".to_string()))
            .subtitle(if summary.is_empty() {
                "Ingen systemdata".to_string()
            } else {
                summary.join(" · ")
            })
            .build(),
    );

    if let Some(uptime) = snap.uptime_seconds {
        rows.push(
            adw::ActionRow::builder()
                .title("Drifttid")
                .subtitle(format_uptime(uptime))
                .build(),
        );
    }
    if let Some(load) = snap.load {
        rows.push(
            adw::ActionRow::builder()
                .title("Systemlast")
                .subtitle(format!(
                    "{:.2} / {:.2} / {:.2} (1/5/15 min)",
                    load.one, load.five, load.fifteen
                ))
                .build(),
        );
    }
    if let Some(mem) = snap.memory {
        rows.push(
            adw::ActionRow::builder()
                .title("Minne")
                .subtitle(format!(
                    "{} / {} ({:.0} %)",
                    format_bytes(mem.used_bytes()),
                    format_bytes(mem.total_bytes),
                    mem.used_fraction() * 100.0
                ))
                .build(),
        );
    }
    for disk in &snap.disks {
        rows.push(
            adw::ActionRow::builder()
                .title(disk.mount.clone())
                .subtitle(format!(
                    "{} / {} ({} %) — {}",
                    format_bytes(disk.used_bytes),
                    format_bytes(disk.size_bytes),
                    disk.capacity_percent,
                    disk.filesystem
                ))
                .build(),
        );
    }
    for container in &snap.containers {
        let row = adw::ActionRow::builder()
            .title(container.name.clone())
            .subtitle(format!("{} — {}", container.image, container.status))
            .build();
        let icon_name = if container.is_running() {
            "media-playback-start-symbolic"
        } else {
            "media-playback-stop-symbolic"
        };
        row.add_prefix(&gtk::Image::from_icon_name(icon_name));
        rows.push(row);
    }

    if !snap.temperatures.is_empty() {
        let mut temperatures = snap.temperatures.clone();
        temperatures.sort_by(|left, right| {
            right
                .celsius
                .partial_cmp(&left.celsius)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let summary: Vec<String> = temperatures
            .iter()
            .take(4)
            .map(|temperature| {
                format!("{}: {:.1} °C", temperature.label, temperature.celsius)
            })
            .collect();
        let mut subtitle = summary.join(" · ");
        if temperatures.len() > 4 {
            subtitle.push_str(&format!(" · +{} till", temperatures.len() - 4));
        }
        rows.push(
            adw::ActionRow::builder()
                .title("Temperatur")
                .subtitle(subtitle)
                .build(),
        );
    }

    for address in &snap.addresses {
        rows.push(
            adw::ActionRow::builder()
                .title(address.address.clone())
                .subtitle(format!(
                    "{} · {}",
                    address.interface,
                    if address.is_ipv6 { "IPv6" } else { "IPv4" }
                ))
                .build(),
        );
    }

    for key in &snap.authorized_keys {
        let who = if key.comment.is_empty() {
            "utan kommentar"
        } else {
            key.comment.as_str()
        };
        rows.push(
            adw::ActionRow::builder()
                .title(who)
                .subtitle(format!(
                    "{} {} bitar · {}",
                    key.algorithm, key.bits, key.fingerprint
                ))
                .build(),
        );
    }

    for user in &snap.active_users {
        let mut subtitle = format!("{} · sedan {}", user.tty, user.since);
        match &user.from {
            Some(from) => subtitle.push_str(&format!(" · från {from}")),
            None => subtitle.push_str(" · lokalt"),
        }
        rows.push(
            adw::ActionRow::builder()
                .title(user.user.clone())
                .subtitle(subtitle)
                .build(),
        );
    }

    rows
}
