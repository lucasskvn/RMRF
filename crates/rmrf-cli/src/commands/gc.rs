use rmrf_manifest::Manifest;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::helpers::{format_size, hex_to_bytes};

struct GcStats {
    orphans: u64,
    freed_bytes: u64,
}

pub fn gc(store_dir: &Path, dry_run: bool) -> bool {
    if !store_dir.exists() {
        eprintln!("Error: store directory not found: {}", store_dir.display());
        return false;
    }
    let referenced = match collect_referenced_hashes(store_dir) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: {e}");
            eprintln!("Aborting GC: fix or remove the unreadable manifest before retrying.");
            return false;
        }
    };
    let stats = scan_and_delete_orphans(store_dir, &referenced, dry_run);
    print_gc_report(&stats, dry_run);
    true
}

fn collect_referenced_hashes(store_dir: &Path) -> Result<HashSet<[u8; 32]>, String> {
    let manifests_dir = store_dir.join("manifests");
    let mut referenced = HashSet::new();
    let entries = match fs::read_dir(&manifests_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(referenced),
        Err(e) => return Err(format!("cannot read manifests directory: {e}")),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "bincode") {
            continue;
        }
        let manifest = Manifest::load(&path)
            .map_err(|e| format!("manifest {} is unreadable: {e}", path.display()))?;
        for hash in manifest.chunks {
            referenced.insert(hash);
        }
    }
    Ok(referenced)
}

fn scan_and_delete_orphans(store_dir: &Path,referenced: &HashSet<[u8; 32]>,dry_run: bool) -> GcStats {
    let chunks_dir = store_dir.join("chunks");
    let hex_char = |b: u8| b.is_ascii_hexdigit();
    let mut orphans = 0u64;
    let mut freed_bytes = 0u64;

    if let Ok(entries) = fs::read_dir(&chunks_dir) {
        for prefix in entries.flatten() {
            let (_name, deleted) = scan_prefix(&prefix, hex_char, referenced, dry_run);
            orphans += deleted.orphans;
            freed_bytes += deleted.freed_bytes;

            if !dry_run
                && prefix
                    .path()
                    .read_dir()
                    .map_or(true, |mut d| d.next().is_none())
            {
                fs::remove_dir(prefix.path()).ok();
            }
        }
    }

    GcStats {
        orphans,
        freed_bytes,
    }
}

fn scan_prefix(prefix: &fs::DirEntry,hex_char: impl Fn(u8) -> bool,referenced: &HashSet<[u8; 32]>,dry_run: bool) -> (String, GcStats) {
    let name = prefix.file_name().to_string_lossy().to_string();
    let mut stats = GcStats {
        orphans: 0,
        freed_bytes: 0,
    };

    if name.len() != 2 || !name.bytes().all(hex_char) {
        return (name, stats);
    }
    if let Ok(inner_entries) = fs::read_dir(prefix.path()) {
        for inner in inner_entries.flatten() {
            let inner_name = inner.file_name().to_string_lossy().to_string();
            try_delete_orphan(&inner, &inner_name, &name, referenced, dry_run, &mut stats);
        }
    }

    (name, stats)
}

fn try_delete_orphan(inner: &fs::DirEntry, inner_name: &str,name: &str,referenced: &HashSet<[u8; 32]>,dry_run: bool,stats: &mut GcStats) {
    let hex_str = format!("{name}{inner_name}");
    let Some(hash_bytes) = hex_to_bytes(&hex_str) else {
        eprintln!("Warning: non-hex file in chunks dir: {name}/{inner_name}");
        return;
    };
    if hash_bytes.len() != 32 {
        eprintln!("Warning: invalid hash length in chunks dir: {name}/{inner_name}");
        return;
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash_bytes);
    if referenced.contains(&hash) {
        return;
    }
    let size = inner.metadata().map(|m| m.len()).unwrap_or(0);
    if dry_run {
        println!(
            "Would delete orphan: {name}/{inner_name} ({})",
            format_size(size)
        );
    } else {
        if let Err(e) = fs::remove_file(inner.path()) {
            eprintln!("Error deleting {name}/{inner_name}: {e}");
            return;
        }
        println!(
            "Deleted orphan: {name}/{inner_name} ({})",
            format_size(size)
        );
    }
    stats.orphans += 1;
    stats.freed_bytes += size;
}

fn print_gc_report(stats: &GcStats, dry_run: bool) {
    if dry_run {
        println!(
            "Dry run: would delete {} orphan chunks, would free {}",
            stats.orphans,
            format_size(stats.freed_bytes)
        );
    } else {
        println!(
            "Deleted {} orphan chunks, freed {}",
            stats.orphans,
            format_size(stats.freed_bytes)
        );
    }
}
