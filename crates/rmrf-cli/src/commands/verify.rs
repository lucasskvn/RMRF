use rmrf_core::{ChunkStore, Hash};
use rmrf_manifest::Manifest;
use rmrf_store::hash_data;
use std::path::Path;

pub fn verify(manifest_path: &Path,store_dir: &Path, rehash: bool,identity: Option<&str>) -> bool {
    if !store_dir.exists() {
        eprintln!("Error: store directory not found: {}", store_dir.display());
        return false;
    }
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return false;
        }
    };
    let store = match crate::restore::open_chunk_store(store_dir, &manifest, identity) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error opening chunk store: {e}");
            return false;
        }
    };
    let (total, missing, corrupted) = check_all_chunks(&manifest, &*store, rehash);

    report_result(total, missing, corrupted, rehash)
}

fn report_result(total: usize, missing: u64, corrupted: u64, rehash: bool) -> bool {
    if missing == 0 && corrupted == 0 {
        if rehash {
            println!("OK: all {total} chunks present and verified (re-hashed)");
        } else {
            println!("OK: all {total} chunks present and verified");
        }
        return true;
    }
    if missing > 0 {
        eprintln!("FAIL: {missing}/{total} chunks missing");
    }
    if corrupted > 0 {
        eprintln!("FAIL: {corrupted}/{total} chunks corrupted (hash mismatch)");
    }
    false
}

fn check_all_chunks(manifest: &Manifest, store: &dyn ChunkStore,rehash: bool) -> (usize, u64, u64) {
    let total = manifest.chunks.len();
    let mut missing = 0u64;
    let mut corrupted = 0u64;

    for hash in &manifest.chunks {
        if rehash {
            match check_chunk_integrity(store, hash) {
                CheckResult::Ok => {}
                CheckResult::Missing | CheckResult::Error => missing += 1,
                CheckResult::Corrupted => corrupted += 1,
            }
        } else if !check_chunk_exists(store, hash) {
            missing += 1;
        }
    }
    (total, missing, corrupted)
}

enum CheckResult {
    Ok,
    Missing,
    Corrupted,
    Error,
}

fn check_chunk_integrity(store: &dyn ChunkStore, hash: &Hash) -> CheckResult {
    match store.get(hash) {
        Ok(Some(data)) => {
            if &hash_data(&data) == hash {
                CheckResult::Ok
            } else {
                eprintln!("Corrupted chunk: {:02x?}... (hash mismatch)", &hash[..8]);
                CheckResult::Corrupted
            }
        }
        Ok(None) => {
            eprintln!("Missing chunk: {:02x?}...", &hash[..8]);
            CheckResult::Missing
        }
        Err(e) => {
            eprintln!("Error reading chunk: {e}");
            CheckResult::Error
        }
    }
}

fn check_chunk_exists(store: &dyn ChunkStore, hash: &Hash) -> bool {
    match store.exists(hash) {
        Ok(true) => true,
        Ok(false) => {
            eprintln!("Missing chunk: {:02x?}...", &hash[..8]);
            false
        }
        Err(e) => {
            eprintln!("Error checking chunk: {e}");
            false
        }
    }
}
