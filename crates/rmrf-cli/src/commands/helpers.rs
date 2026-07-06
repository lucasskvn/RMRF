use std::path::Path;

pub fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GiB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

pub fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

pub fn parse_timestamp(s: &str) -> i64 {
    if s.len() < 15 {
        return 0;
    }
    let year: i32 = s[..4].parse().unwrap_or(1970);
    let month: u32 = s[4..6].parse().unwrap_or(1);
    let day: u32 = s[6..8].parse().unwrap_or(1);
    let hour: u32 = s[9..11].parse().unwrap_or(0);
    let min: u32 = s[11..13].parse().unwrap_or(0);
    let sec: u32 = s[13..15].parse().unwrap_or(0);
    let days = days_from_civil(year, month, day) - days_from_civil(1970, 1, 1);
    days as i64 * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = y - (m <= 2) as i32;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (m + if m > 2 { 3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

#[allow(dead_code)]
pub fn join_display(path: &Path) -> String {
    path.display().to_string()
}