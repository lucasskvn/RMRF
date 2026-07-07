use rmrf_manifest::Manifest;
use std::fs;
use std::path::Path;

use super::helpers::format_size;

pub fn list(
    store_dir: &Path,
    since: Option<&str>,
    before: Option<&str>,
    source_filter: Option<&str>,
) -> bool {
    let manifests_dir = store_dir.join("manifests");
    let mut entries: Vec<_> = match fs::read_dir(&manifests_dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            eprintln!("Error reading manifests directory: {e}");
            return false;
        }
    };
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("No backups found.");
        return true;
    }

    let since_parsed = since.and_then(parse_date_prefix);
    let before_parsed = before.and_then(parse_date_prefix);

    let mut count = 0u64;
    for entry in &entries {
        if print_manifest_entry(entry, since_parsed, before_parsed, source_filter) {
            count += 1;
        }
    }
    if count == 0 {
        println!("No backups match the given filters.");
    }
    true
}

fn parse_date_prefix(s: &str) -> Option<u64> {
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() >= 8 {
        digits[..8].parse::<u64>().ok()
    } else {
        None
    }
}

fn print_manifest_entry(
    entry: &fs::DirEntry,
    since: Option<u64>,
    before: Option<u64>,
    source_filter: Option<&str>,
) -> bool {
    let path = entry.path();
    if path.extension().is_none_or(|ext| ext != "bincode") {
        return false;
    }
    let basename = path.file_name().unwrap_or_default().to_string_lossy();
    let ts_str = basename.split('-').next().unwrap_or("unknown");
    let ts_num = ts_str.parse::<u64>().unwrap_or(0);

    if let Some(s) = since {
        if ts_num < s {
            return false;
        }
    }
    if let Some(b) = before {
        if ts_num >= b {
            return false;
        }
    }

    let manifest = match Manifest::load(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Warning: could not load {}: {}", path.display(), e);
            return false;
        }
    };

    if let Some(filter) = source_filter {
        if !manifest.source.path.contains(filter) {
            return false;
        }
    }

    let size = format_size(manifest.source.size);
    println!("{}  {:>9}  {:>4} chunks  {}",
        ts_str, size, manifest.chunks.len(), manifest.source.path);
    true
}
