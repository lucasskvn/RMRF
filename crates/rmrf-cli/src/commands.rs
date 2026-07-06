use rmrf_core::{ChunkStore, DedupIndex};
use rmrf_manifest::Manifest;
use rmrf_store::{CompressedChunkStore, LocalChunkStore, PersistentDedupIndex};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub fn list(store_dir: &Path) {
    let manifests_dir = store_dir.join("manifests");
    let mut entries: Vec<_> = match fs::read_dir(&manifests_dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            eprintln!("Error reading manifests directory: {e}");
            return;
        }
    };
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("No backups found.");
        return;
    }

    for entry in &entries {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "bincode") {
            continue;
        }
        let manifest = match Manifest::load(&path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Warning: could not load {}: {}", path.display(), e);
                continue;
            }
        };
        let basename = path.file_name().unwrap().to_string_lossy();
        let timestamp = basename.split('-').next().unwrap_or("unknown");
        println!(
            "{}  {:>9}  {:>4} chunks  {}",
            timestamp,
            format_size(manifest.source.size),
            manifest.chunks.len(),
            manifest.source.path,
        );
    }
}

pub fn info(manifest_path: &Path, _store_dir: &Path) {
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return;
        }
    };
    println!("Version:     v{}", manifest.version);
    println!("Source path: {}", manifest.source.path);
    println!(
        "Source size: {} ({})",
        manifest.source.size,
        format_size(manifest.source.size)
    );
    println!("Source type: {:?}", manifest.source.source_type);
    println!("Modified:    {}", manifest.source.modified);
    println!("Permissions: {:o}", manifest.source.permissions);
    println!("Chunks:      {}", manifest.chunks.len());
}

pub fn verify(manifest_path: &Path, store_dir: &Path) {
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return;
        }
    };
    let chunks_dir = store_dir.join("chunks");
    let local_store = match LocalChunkStore::new(chunks_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error opening chunk store: {e}");
            return;
        }
    };
    let store = CompressedChunkStore::new(local_store);

    let total = manifest.chunks.len();
    let mut missing = 0u64;

    for hash in &manifest.chunks {
        match store.exists(hash) {
            Ok(true) => {}
            Ok(false) => {
                missing += 1;
                eprintln!("Missing chunk: {:02x?}...", &hash[..8]);
            }
            Err(e) => {
                eprintln!("Error checking chunk: {e}");
                missing += 1;
            }
        }
    }

    if missing == 0 {
        println!("OK: all {total} chunks present and verified");
    } else {
        eprintln!("FAIL: {missing}/{total} chunks missing");
        std::process::exit(1);
    }
}

pub fn gc(store_dir: &Path, dry_run: bool) {
    let manifests_dir = store_dir.join("manifests");
    let mut referenced: HashSet<[u8; 32]> = HashSet::new();

    if let Ok(entries) = fs::read_dir(&manifests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "bincode") {
                continue;
            }
            if let Ok(m) = Manifest::load(&path) {
                for hash in m.chunks {
                    referenced.insert(hash);
                }
            }
        }
    }

    let chunks_dir = store_dir.join("chunks");
    let hex_char = |b: u8| b.is_ascii_hexdigit();
    let mut orphans = 0u64;
    let mut freed_bytes = 0u64;

    if let Ok(entries) = fs::read_dir(&chunks_dir) {
        for prefix in entries.flatten() {
            let name = prefix.file_name().to_string_lossy().to_string();
            if name.len() != 2 || !name.bytes().all(hex_char) {
                continue;
            }
            if let Ok(inner_entries) = fs::read_dir(prefix.path()) {
                for inner in inner_entries.flatten() {
                    let inner_name = inner.file_name().to_string_lossy().to_string();
                    let hex_str = format!("{}{}", name, inner_name);
                    if let Some(hash_bytes) = hex_to_bytes(&hex_str) {
                        if hash_bytes.len() == 32 {
                            let mut hash = [0u8; 32];
                            hash.copy_from_slice(&hash_bytes);
                            if !referenced.contains(&hash) {
                                let size = inner.metadata().map(|m| m.len()).unwrap_or(0);
                                if dry_run {
                                    println!(
                                        "Would delete orphan: {}/{inner_name} ({})",
                                        name,
                                        format_size(size)
                                    );
                                } else {
                                    fs::remove_file(inner.path()).ok();
                                    println!(
                                        "Deleted orphan: {}/{inner_name} ({})",
                                        name,
                                        format_size(size)
                                    );
                                }
                                orphans += 1;
                                freed_bytes += size;
                            }
                        }
                    }
                }
            }
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

    if dry_run {
        println!(
            "Dry run: would delete {orphans} orphan chunks, would free {}",
            format_size(freed_bytes)
        );
    } else {
        println!(
            "Deleted {orphans} orphan chunks, freed {}",
            format_size(freed_bytes)
        );
    }
}

pub fn run_retention_and_gc(store_dir: &Path, days: u64) {
    let manifests_dir = store_dir.join("manifests");
    let now = chrono::Utc::now().timestamp();
    let cutoff = now - (days as i64) * 86400;

    let index_path = store_dir.join("index.bincode");
    let mut dedup_index = match PersistentDedupIndex::new(index_path) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Warning: failed to load dedup index for retention: {e}");
            return;
        }
    };

    let mut deleted_manifests = 0u64;
    if let Ok(entries) = fs::read_dir(&manifests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "bincode") {
                continue;
            }
            let basename = path.file_name().unwrap().to_string_lossy();
            let ts_str = basename.split('-').next().unwrap_or("");
            let ts = parse_timestamp(ts_str);
            if ts < cutoff {
                if let Ok(m) = Manifest::load(&path) {
                    for hash in &m.chunks {
                        let _ = dedup_index.decrement_refcount(hash);
                    }
                    fs::remove_file(&path).ok();
                    deleted_manifests += 1;
                    println!("Removed manifest: {}", path.display());
                }
            }
        }
    }

    if deleted_manifests > 0 {
        let _ = dedup_index.save();
        println!("Retention: removed {deleted_manifests} manifest(s) older than {days} day(s)");
        gc(store_dir, false);
    } else {
        println!("Retention: no manifests older than {days} day(s)");
    }
}

pub fn clean() {
    let artifacts = [
        "empty", "one_byte", "zeros", "ff_bytes", "repeated", "exact_chunk", "chunk_plus",
        "restored", "rmrf_src", "test.txt", "input.txt", "input.bin", "rmrf_input.txt",
        "-rmrf_input.txt", "écoß测试.txt",
    ];

    let mut removed = 0;
    for name in &artifacts {
        let path = Path::new(name);
        if path.exists() {
            match fs::remove_file(path) {
                Ok(_) => {
                    println!("Removed: {name}");
                    removed += 1;
                }
                Err(e) => eprintln!("Failed to remove {name}: {e}"),
            }
        }
    }

    if removed == 0 {
        println!("Nothing to clean.");
    } else {
        println!("Cleaned {removed} test artifact(s).");
    }
}

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

fn parse_timestamp(s: &str) -> i64 {
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
