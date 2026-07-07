use rmrf_core::{ChunkStore, Hash};
use rmrf_manifest::Manifest;
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore};
use std::path::Path;

pub fn verify(manifest_path: &Path, store_dir: &Path, rehash: bool) -> bool {
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return false;
        }
    };

    let store = match open_compressed_store(store_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error opening chunk store: {e}");
            return false;
        }
    };

    let (total, missing, corrupted) = check_all_chunks(&manifest, &store, rehash);

    if missing == 0 && corrupted == 0 {
        if rehash {
            println!("OK: all {total} chunks present and verified (re-hashed)");
        } else {
            println!("OK: all {total} chunks present and verified");
        }
        true
    } else {
        if missing > 0 {
            eprintln!("FAIL: {missing}/{total} chunks missing");
        }
        if corrupted > 0 {
            eprintln!("FAIL: {corrupted}/{total} chunks corrupted (hash mismatch)");
        }
        false
    }
}

fn open_compressed_store(
    store_dir: &Path,
) -> Result<CompressedChunkStore<LocalChunkStore>, String> {
    let chunks_dir = store_dir.join("chunks");
    let local_store = LocalChunkStore::new(chunks_dir).map_err(|e| e.to_string())?;
    Ok(CompressedChunkStore::new(local_store))
}

fn check_all_chunks(
    manifest: &Manifest,
    store: &CompressedChunkStore<LocalChunkStore>,
    rehash: bool,
) -> (usize, u64, u64) {
    let total = manifest.chunks.len();
    let mut missing = 0u64;
    let mut corrupted = 0u64;

    for hash in &manifest.chunks {
        if rehash {
            match check_chunk_integrity(store, hash) {
                CheckResult::Ok => {}
                CheckResult::Missing => missing += 1,
                CheckResult::Corrupted => corrupted += 1,
                CheckResult::Error => {
                    missing += 1;
                }
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

fn check_chunk_integrity(
    store: &CompressedChunkStore<LocalChunkStore>,
    hash: &Hash,
) -> CheckResult {
    match store.get(hash) {
        Ok(Some(data)) => {
            let computed = hash_data(&data);
            if &computed == hash {
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

fn check_chunk_exists(
    store: &CompressedChunkStore<LocalChunkStore>,
    hash: &Hash,
) -> bool {
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
