use rmrf_core::DedupIndex;
use rmrf_manifest::Manifest;
use rmrf_store::PersistentDedupIndex;
use std::fs;
use std::path::Path;

use super::gc::gc;
use super::helpers::parse_timestamp;

pub fn run_retention_and_gc(store_dir: &Path, days: u64) {
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

    let deleted = remove_old_manifests(store_dir, cutoff, &mut dedup_index);

    if deleted > 0 {
        let _ = dedup_index.save();
        println!("Retention: removed {deleted} manifest(s) older than {days} day(s)");
        gc(store_dir, false);
    } else {
        println!("Retention: no manifests older than {days} day(s)");
    }
}

fn remove_old_manifests(
    store_dir: &Path,
    cutoff: i64,
    dedup_index: &mut PersistentDedupIndex,
) -> u64 {
    let manifests_dir = store_dir.join("manifests");
    let mut deleted = 0u64;

    let Ok(entries) = fs::read_dir(&manifests_dir) else {
        return 0;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "bincode") {
            continue;
        }
        let basename = path.file_name().unwrap_or_default().to_string_lossy();
        let ts_str = basename.split('-').next().unwrap_or("");
        if parse_timestamp(ts_str) >= cutoff {
            continue;
        }
        if decref_manifest(&path, dedup_index) {
            fs::remove_file(&path).ok();
            deleted += 1;
            println!("Removed manifest: {}", path.display());
        }
    }

    deleted
}

fn decref_manifest(path: &Path, dedup_index: &mut PersistentDedupIndex) -> bool {
    let Ok(m) = Manifest::load(path) else {
        return false;
    };
    for hash in &m.chunks {
        let _ = dedup_index.decrement_refcount(hash);
    }
    true
}