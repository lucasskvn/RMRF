use fastcdc::v2016::StreamCDC;
use indicatif::{ProgressBar, ProgressStyle};
use rmrf_core::{ChunkStore, DedupIndex, DiskFormat, Source, SourceMetadata};
use rmrf_manifest::Manifest;
use rmrf_source::{FileSource, VmLifecycle, VmSource};
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore, PersistentDedupIndex};
use std::fs;
use std::io;
use std::path::Path;

pub fn backup(source: &Path, store_dir: &Path) {
    if source.is_dir() {
        backup_dir(source, store_dir);
    } else {
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let basename = source
            .file_name()
            .unwrap_or(source.as_os_str())
            .to_string_lossy()
            .to_string();
        backup_file(source, store_dir, &timestamp, &basename);
    }
}

#[derive(Default)]
struct DirStats {
    chunks: u64,
    bytes: u64,
    skipped: u64,
    files: u64,
}

impl DirStats {
    fn add(&mut self, s: BackupStats) {
        self.chunks += s.chunks;
        self.bytes += s.bytes;
        self.skipped += s.dedup_skipped;
        self.files += 1;
    }
}

fn compute_rel_label(path: &Path, dir: &Path) -> String {
    path.strip_prefix(dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('/', "_")
}

fn backup_dir(dir: &Path, store_dir: &Path) {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    println!("Backup directory: {}", dir.display());

    let files = match walk_files(dir) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error reading directory: {e}");
            return;
        }
    };

    println!("Found {} files", files.len());
    let mut stats = DirStats::default();
    for path in &files {
        let rel = compute_rel_label(path, dir);
        stats.add(backup_file(path, store_dir, &timestamp, &rel));
    }

    println!("Directory backup complete!");
    print_dir_summary(stats.files, stats.chunks, stats.bytes, stats.skipped);
}

fn print_dir_summary(files: u64, chunks: u64, bytes: u64, skipped: u64) {
    println!("Files: {files}");
    println!("Chunks: {chunks}");
    println!("Total bytes: {bytes}");
    println!("Dedup skipped (already stored): {skipped}");
}

fn walk_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, io::Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let name = path.file_name();
        if name.is_none_or(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        if path.is_dir() {
            files.extend(walk_files(&path)?);
        } else if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn backup_file(source: &Path, store_dir: &Path, timestamp: &str, label: &str) -> BackupStats {
    println!("Backup: {}", source.display());

    let mut file_source = match FileSource::new(source) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error opening file: {e}");
            return BackupStats { chunks: 0, bytes: 0, dedup_skipped: 0 };
        }
    };

    let metadata = file_source.metadata();
    run_chunk_pipeline(&mut file_source, metadata, store_dir, timestamp, label)
}

pub fn backup_vm(
    path: &std::path::Path,
    store_dir: &std::path::Path,
    vm_id: &str,
    disk_name: &str,
    format: DiskFormat,
    lifecycle: Box<dyn VmLifecycle>,
) {
    let mut vm_source = match VmSource::open(path, vm_id, disk_name, format, lifecycle) {
        Ok(vs) => vs,
        Err(e) => {
            eprintln!("Error opening VM source: {e}");
            return;
        }
    };

    let metadata = vm_source.metadata();
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let basename = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .to_string();

    run_chunk_pipeline(
        &mut vm_source,
        metadata,
        store_dir,
        &timestamp,
        &basename,
    );
}

fn run_chunk_pipeline(
    source: &mut dyn Source,
    metadata: SourceMetadata,
    store_dir: &Path,
    timestamp: &str,
    label: &str,
) -> BackupStats {
    println!("Source size: {} bytes", metadata.size);

    let chunks_dir = store_dir.join("chunks");
    let _ = fs::create_dir_all(&chunks_dir);
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

struct ChunkInfo {
    hash: [u8; 32],
    data_len: usize,
    was_duplicate: bool,
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

fn finish_backup(
    chunk_hashes: &[[u8; 32]],
    dedup_index: &mut PersistentDedupIndex,
    metadata: SourceMetadata,
    store_dir: &Path,
    timestamp: &str,
    label: &str,
    stats: BackupStats,
) -> BackupStats {
    let _ = dedup_index.save();
    save_manifest(chunk_hashes, metadata, store_dir, timestamp, label, stats)
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

enum ProcessResult {
    Stored,
    Skipped,
    Error(String),
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

fn process_chunk(
    hash: &[u8; 32],
    data: &[u8],
    chunk_store: &mut CompressedChunkStore<LocalChunkStore>,
    dedup_index: &mut PersistentDedupIndex,
) -> ProcessResult {
    if dedup_index.lookup(hash).ok().flatten().is_some() {
        let _ = dedup_index.increment_refcount(hash);
        return ProcessResult::Skipped;
    }
    let uncompressed = data.len() as u32;
    match chunk_store.put(hash, data) {
        Ok(location) => {
            let _ = dedup_index.insert(hash, location, uncompressed, uncompressed);
            ProcessResult::Stored
        }
        Err(e) => ProcessResult::Error(e.to_string()),
    }
}

struct BackupStats {
    chunks: u64,
    bytes: u64,
    dedup_skipped: u64,
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
    let _ = fs::create_dir_all(&manifests_dir);
    let manifest_path = manifests_dir.join(format!("{timestamp}-{label}.bincode"));

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