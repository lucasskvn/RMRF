use fastcdc::v2016::StreamCDC;
use indicatif::{ProgressBar, ProgressStyle};
use rmrf_core::{ChunkStore, DedupIndex, Source, SourceMetadata};
use rmrf_manifest::Manifest;
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore, PersistentDedupIndex};
use std::fs;
use std::path::Path;

pub struct BackupStats {
    pub chunks: u64,
    pub bytes: u64,
    pub dedup_skipped: u64,
}

struct ChunkInfo {
    hash: [u8; 32],
    data_len: usize,
    was_duplicate: bool,
}

enum ProcessResult {
    Stored,
    Skipped,
    Error(String),
}

pub fn run_chunk_pipeline(
    source: &mut dyn Source,
    metadata: SourceMetadata,
    store_dir: &Path,
    timestamp: &str,
    label: &str,
) -> BackupStats {
    println!("Source size: {} bytes", metadata.size);

    let chunks_dir = store_dir.join("chunks");
    if let Err(e) = fs::create_dir_all(&chunks_dir) {
        eprintln!("Error creating chunks directory: {e}");
        return BackupStats { chunks: 0, bytes: 0, dedup_skipped: 0 };
    }
    let local_store = match LocalChunkStore::new(chunks_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error creating chunk store: {e}");
            return BackupStats { chunks: 0, bytes: 0, dedup_skipped: 0 };
        }
    };

    let mut chunk_store = CompressedChunkStore::new(local_store);
    let index_path = store_dir.join("index.bincode");
    let mut dedup_index = match PersistentDedupIndex::new(index_path) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: failed to load dedup index: {e}");
            return BackupStats { chunks: 0, bytes: 0, dedup_skipped: 0 };
        }
    };

    run_chunk_loop(
        source,
        &mut chunk_store,
        &mut dedup_index,
        store_dir,
        metadata,
        timestamp,
        label,
    )
}

fn run_chunk_loop(
    source: &mut dyn Source,
    chunk_store: &mut CompressedChunkStore<LocalChunkStore>,
    dedup_index: &mut PersistentDedupIndex,
    store_dir: &Path,
    metadata: SourceMetadata,
    timestamp: &str,
    label: &str,
) -> BackupStats {
    let chunker = StreamCDC::new(source, 4096, 16384, 65536);
    let mut chunk_hashes = Vec::new();
    let mut chunk_count = 0u64;
    let mut total_bytes = 0u64;
    let mut dedup_skipped = 0u64;
    let pb = create_backup_spinner();

    for chunk_result in chunker {
        match feed_chunk(chunk_result, chunk_store, dedup_index) {
            Ok(info) => {
                chunk_hashes.push(info.hash);
                chunk_count += 1;
                total_bytes += info.data_len as u64;
                if info.was_duplicate { dedup_skipped += 1; }
                pb.inc(1);
                pb.set_message(format!("{dedup_skipped}"));
                pb.set_position(chunk_count);
            }
            Err(e) => {
                pb.finish_and_clear();
                eprintln!("{e}");
                return BackupStats { chunks: 0, bytes: 0, dedup_skipped: 0 };
            }
        }
    }

    pb.finish_and_clear();
    finish_backup(&chunk_hashes, dedup_index, metadata, store_dir, timestamp, label,
                  BackupStats { chunks: chunk_count, bytes: total_bytes, dedup_skipped })
}

fn feed_chunk(
    result: std::result::Result<fastcdc::v2016::ChunkData, fastcdc::v2016::Error>,
    chunk_store: &mut CompressedChunkStore<LocalChunkStore>,
    dedup_index: &mut PersistentDedupIndex,
) -> std::result::Result<ChunkInfo, String> {
    let chunk = result.map_err(|e| format!("Error reading chunk: {e}"))?;
    let data = &chunk.data;
    let hash = hash_data(data);
    let was_duplicate = match process_chunk(&hash, data, chunk_store, dedup_index) {
        ProcessResult::Skipped => true,
        ProcessResult::Stored => false,
        ProcessResult::Error(e) => return Err(e),
    };
    Ok(ChunkInfo { hash, data_len: data.len(), was_duplicate })
}

fn process_chunk(
    hash: &[u8; 32],
    data: &[u8],
    chunk_store: &mut CompressedChunkStore<LocalChunkStore>,
    dedup_index: &mut PersistentDedupIndex,
) -> ProcessResult {
    match dedup_index.lookup(hash) {
        Ok(Some(_)) => {
            match chunk_store.exists(hash) {
                Ok(true) => {
                    if let Err(e) = dedup_index.increment_refcount(hash) {
                        return ProcessResult::Error(format!("Error incrementing refcount: {e}"));
                    }
                    return ProcessResult::Skipped;
                }
                Ok(false) => {}
                Err(e) => {
                    return ProcessResult::Error(format!("Error checking chunk existence: {e}"));
                }
            }
        }
        Err(e) => {
            return ProcessResult::Error(format!("Error looking up dedup index: {e}"));
        }
        Ok(None) => {}
    }
    let uncompressed = data.len() as u32;
    match chunk_store.put(hash, data) {
        Ok(location) => {
            let compressed_raw = match chunk_store.get_raw(hash) {
                Ok(d) => d,
                Err(e) => return ProcessResult::Error(format!("Failed to get compressed size: {e}")),
            };
            let compressed_size = compressed_raw
                .map(|d| d.len() as u32)
                .unwrap_or(uncompressed);
            if let Err(e) = dedup_index.insert(hash, location, uncompressed, compressed_size) {
                return ProcessResult::Error(format!("Error inserting into dedup index: {e}"));
            }
            ProcessResult::Stored
        }
        Err(e) => ProcessResult::Error(e.to_string()),
    }
}

fn finish_backup(
    chunk_hashes: &[[u8; 32]],
    dedup_index: &mut PersistentDedupIndex,
    metadata: SourceMetadata,
    store_dir: &Path,
    timestamp: &str,
    label: &str,
    stats: BackupStats,
) -> BackupStats {
    if let Err(e) = dedup_index.save() {
        eprintln!("Warning: failed to save dedup index: {e}");
    }
    save_manifest(chunk_hashes, metadata, store_dir, timestamp, label, stats)
}

fn save_manifest(
    chunk_hashes: &[[u8; 32]],
    metadata: SourceMetadata,
    store_dir: &Path,
    timestamp: &str,
    label: &str,
    stats: BackupStats,
) -> BackupStats {
    let manifest = Manifest {
        version: rmrf_manifest::MANIFEST_VERSION,
        source: metadata,
        chunks: chunk_hashes.to_vec(),
    };
    let manifests_dir = store_dir.join("manifests");
    if let Err(e) = fs::create_dir_all(&manifests_dir) {
        eprintln!("Error creating manifests directory: {e}");
        return stats;
    }
    let mut manifest_path = manifests_dir.join(format!("{timestamp}-{label}.bincode"));
    let mut collision = 0u64;
    while manifest_path.exists() {
        collision += 1;
        manifest_path = manifests_dir.join(format!("{timestamp}-{label}-{collision}.bincode"));
    }

    match manifest.save(&manifest_path) {
        Ok(_) => println!("\nManifest saved: {}", manifest_path.display()),
        Err(e) => eprintln!("Error saving manifest: {e}"),
    }
    println!("Backup complete!");
    println!("Chunks: {}", stats.chunks);
    println!("Total bytes: {}", stats.bytes);
    println!("Dedup skipped (already stored): {}", stats.dedup_skipped);
    stats
}

fn create_backup_spinner() -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner} Chunks: {pos} | Bytes: {bytes} | Dedup: {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈"),
    );
    pb
}
