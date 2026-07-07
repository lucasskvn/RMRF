use rmrf_core::{ChunkStore, Hash};
use rmrf_manifest::Manifest;
use rmrf_store::{CompressedChunkStore, LocalChunkStore};
use std::path::Path;

pub fn verify(manifest_path: &Path, store_dir: &Path) -> bool {
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

    let (total, missing) = check_all_chunks(&manifest, &store);

    if missing == 0 {
        println!("OK: all {total} chunks present and verified");
        true
    } else {
        eprintln!("FAIL: {missing}/{total} chunks missing");
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
) -> (usize, u64) {
    let total = manifest.chunks.len();
    let mut missing = 0u64;

    for hash in &manifest.chunks {
        if !check_chunk_exists(store, hash) {
            missing += 1;
        }
    }
    (total, missing)
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
