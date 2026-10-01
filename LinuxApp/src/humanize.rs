pub(crate) fn format_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes.max(0) as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

pub(crate) fn format_uptime(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let days = total / 86400;
    let hours = (total % 86400) / 3600;
    let minutes = (total % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_binary_units_and_never_render_negative_values() {
        assert_eq!(format_bytes(-1), "0.0 B");
        assert_eq!(format_bytes(0), "0.0 B");
        assert_eq!(format_bytes(1023), "1023.0 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GiB");
    }

    #[test]
    fn uptime_is_compact_and_clamps_negative_values() {
        assert_eq!(format_uptime(-1.0), "0m");
        assert_eq!(format_uptime(59.9), "0m");
        assert_eq!(format_uptime(60.0), "1m");
        assert_eq!(format_uptime(3_661.0), "1h 1m");
        assert_eq!(format_uptime(90_061.0), "1d 1h 1m");
    }
}
