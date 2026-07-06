use rmrf_core::ChunkStore;
use rmrf_manifest::Manifest;
use rmrf_store::{CompressedChunkStore, LocalChunkStore};
use std::path::Path;

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