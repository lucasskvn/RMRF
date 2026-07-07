use crate::config::RmrfConfig;
use crate::pipeline::run_chunk_pipeline;
use rmrf_core::{ChunkStore, DiskFormat, Source};
use rmrf_source::{FileSource, VmLifecycle, VmSource};
use rmrf_store::{CompressedChunkStore, EncryptedChunkStore, LocalChunkStore};
use std::fs;
use std::io;
use std::path::Path;
use std::str::FromStr;

pub use crate::pipeline::BackupStats;

fn open_chunk_store(
    store_dir: &Path,
    cfg: &RmrfConfig,
    encrypt: Option<&str>,
) -> Result<Box<dyn ChunkStore>, String> {
    let chunks_dir = store_dir.join("chunks");
    fs::create_dir_all(&chunks_dir).map_err(|e| format!("Error creating chunks directory: {e}"))?;
    let local = LocalChunkStore::new(chunks_dir).map_err(|e| e.to_string())?;
    let compressed = CompressedChunkStore::with_level(local, cfg.compression_level);
    if let Some(pubkey_str) = encrypt {
        let recipient = age::x25519::Recipient::from_str(pubkey_str)
            .map_err(|e| format!("Invalid age recipient: {e}"))?;
        let identity = age::x25519::Identity::generate();
        Ok(Box::new(EncryptedChunkStore::new(compressed, recipient, identity)))
    } else {
        Ok(Box::new(compressed))
    }
}

pub fn backup(source: &Path, store_dir: &Path, cfg: &RmrfConfig, encrypt: Option<&str>) -> bool {
    if source.is_dir() {
        backup_dir(source, store_dir, cfg, encrypt)
    } else {
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let basename = source
            .file_name()
            .unwrap_or(source.as_os_str())
            .to_string_lossy()
            .to_string();
        backup_file(source, store_dir, &timestamp, &basename, cfg, encrypt).is_some()
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

fn backup_dir(dir: &Path, store_dir: &Path, cfg: &RmrfConfig, encrypt: Option<&str>) -> bool {
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    println!("Backup directory: {}", dir.display());

    let files = match walk_files(dir) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error reading directory: {e}");
            return false;
        }
    };

    println!("Found {} files", files.len());
    let (stats, errors) = backup_all_files(&files, dir, store_dir, &timestamp, cfg, encrypt);
    if errors > 0 {
        eprintln!("Warning: {errors} file(s) failed to backup");
    }

    println!("Directory backup complete!");
    print_dir_summary(stats.files, stats.chunks, stats.bytes, stats.skipped);
    true
}

fn backup_all_files(
    files: &[std::path::PathBuf],
    dir: &Path,
    store_dir: &Path,
    timestamp: &str,
    cfg: &RmrfConfig,
    encrypt: Option<&str>,
) -> (DirStats, u64) {
    let mut stats = DirStats::default();
    let mut errors = 0u64;
    for path in files {
        let rel = compute_rel_label(path, dir);
        match backup_file(path, store_dir, timestamp, &rel, cfg, encrypt) {
            Some(s) => stats.add(s),
            None => errors += 1,
        }
    }
    (stats, errors)
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
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = path.file_name();
        if name.is_none_or(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        if ft.is_dir() {
            files.extend(walk_files(&path)?);
        } else if ft.is_file() {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn backup_file(
    source: &Path,
    store_dir: &Path,
    timestamp: &str,
    label: &str,
    cfg: &RmrfConfig,
    encrypt: Option<&str>,
) -> Option<BackupStats> {
    println!("Backup: {}", source.display());

    let mut file_source = match FileSource::new(source) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error opening file: {e}");
            return None;
        }
    };

    let mut chunk_store = match open_chunk_store(store_dir, cfg, encrypt) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };

    let metadata = file_source.metadata();
    let file_size = metadata.size;
    let encryption_key = encrypt.map(|s| s.to_string());
    let stats = run_chunk_pipeline(
        &mut file_source, metadata, store_dir, timestamp, label, cfg, &mut *chunk_store,
        encryption_key,
    );
    if stats.chunks == 0 && file_size > 0 {
        eprintln!("Warning: backup produced 0 chunks for non-empty file");
        None
    } else {
        Some(stats)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn backup_vm(
    path: &std::path::Path,
    store_dir: &std::path::Path,
    vm_id: &str,
    disk_name: &str,
    format: DiskFormat,
    lifecycle: Box<dyn VmLifecycle>,
    cfg: &RmrfConfig,
    encrypt: Option<&str>,
) -> bool {
    let mut vm_source = match VmSource::open(path, vm_id, disk_name, format, lifecycle) {
        Ok(vs) => vs,
        Err(e) => {
            eprintln!("Error opening VM source: {e}");
            return false;
        }
    };

    let metadata = vm_source.metadata();
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let basename = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .to_string();

    let mut chunk_store = match open_chunk_store(store_dir, cfg, encrypt) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return false;
        }
    };

    let encryption_key = encrypt.map(|s| s.to_string());
    let stats = run_chunk_pipeline(
        &mut vm_source,
        metadata,
        store_dir,
        &timestamp,
        &basename,
        cfg,
        &mut *chunk_store,
        encryption_key,
    );
    if stats.chunks == 0 {
        eprintln!("Warning: backup produced 0 chunks for VM disk");
        false
    } else {
        true
    }
}
