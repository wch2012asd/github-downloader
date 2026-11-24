//! 大小/速度/剩余时间格式化：与 v1 main.py 工具函数逐字对齐。

/// 字节大小 → 人类可读（B / KB 1 位小数 / MB 1 位小数 / GB 2 位小数）。
pub fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else if size < 1024 * 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// 速度 → 人类可读（B/s 0 位小数 / KB/s 1 位 / MB/s 2 位）。
pub fn format_speed(bps: f64) -> String {
    if bps < 1024.0 {
        format!("{bps:.0} B/s")
    } else if bps < 1024.0 * 1024.0 {
        format!("{:.1} KB/s", bps / 1024.0)
    } else {
        format!("{:.2} MB/s", bps / (1024.0 * 1024.0))
    }
}

/// 剩余秒数 → 人类可读（X秒 / X分XX秒 / X时XX分）。
pub fn format_eta(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds}秒")
    } else if seconds < 3600 {
        format!("{}分{:02}秒", seconds / 60, seconds % 60)
    } else {
        format!("{}时{:02}分", seconds / 3600, (seconds % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_boundaries() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(123 * 1024 * 1024 + 456 * 1024), "123.4 MB");
        assert_eq!(format_size(2 * 1024 * 1024 * 1024), "2.00 GB");
    }

    #[test]
    fn speed_units() {
        assert_eq!(format_speed(512.0), "512 B/s");
        assert_eq!(format_speed(2048.0), "2.0 KB/s");
        assert_eq!(format_speed(1.5 * 1024.0 * 1024.0), "1.50 MB/s");
    }

    #[test]
    fn eta_units() {
        assert_eq!(format_eta(45), "45秒");
        assert_eq!(format_eta(95), "1分35秒");
        assert_eq!(format_eta(3675), "1时01分");
    }
}
