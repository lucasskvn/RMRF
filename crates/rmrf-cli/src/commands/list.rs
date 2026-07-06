use rmrf_manifest::Manifest;
use std::fs;
use std::path::Path;

use super::helpers::format_size;

pub fn list(store_dir: &Path) -> bool {
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

    for entry in &entries {
        print_manifest_entry(entry);
    }
    true
}

fn print_manifest_entry(entry: &fs::DirEntry) {
    let path = entry.path();
    if path.extension().is_none_or(|ext| ext != "bincode") {
        return;
    }
    let manifest = match Manifest::load(&path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Warning: could not load {}: {}", path.display(), e);
            return;
        }
    };
    let basename = path.file_name().unwrap_or_default().to_string_lossy();
    let timestamp = basename.split('-').next().unwrap_or("unknown");
    println!(
        "{}  {:>9}  {:>4} chunks  {}",
        timestamp,
        format_size(manifest.source.size),
        manifest.chunks.len(),
        manifest.source.path,
    );
}
