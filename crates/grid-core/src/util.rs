/// Human-readable byte size formatting (e.g. `"1.50 MB"`).
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_bytes() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn sub_kilobyte() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn exact_kilobyte() {
        assert_eq!(format_bytes(1024), "1.00 KB");
    }

    #[test]
    fn fractional_kilobytes() {
        assert_eq!(format_bytes(1536), "1.50 KB");
    }

    #[test]
    fn exact_megabyte() {
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }

    #[test]
    fn fractional_megabytes() {
        assert_eq!(format_bytes(1024 * 1024 + 512 * 1024), "1.50 MB");
    }

    #[test]
    fn exact_gigabyte() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn multi_gigabyte() {
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn boundary_just_below_kb() {
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn boundary_just_below_mb() {
        let just_below = 1024 * 1024 - 1;
        assert_eq!(format_bytes(just_below), "1024.00 KB");
    }

    #[test]
    fn boundary_just_below_gb() {
        let just_below = 1024 * 1024 * 1024 - 1;
        assert_eq!(format_bytes(just_below), "1024.00 MB");
    }
}
