use fastcdc::v2016::StreamCDC;
use indicatif::{ProgressBar, ProgressStyle};
use rmrf_core::{ChunkStore, DedupIndex, DiskFormat, Source};
use rmrf_manifest::Manifest;
use rmrf_source::{FileSource, VmLifecycle, VmSource};
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore, PersistentDedupIndex};
use std::fs;
use std::io;
use std::path::Path;

pub fn backup(source: &Path, store_dir: &Path)
{
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

pub fn backup_dir(dir: &Path, store_dir: &Path) {
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
    let mut total_chunks = 0u64;
    let mut total_bytes = 0u64;
    let mut total_skipped = 0u64;
    let mut total_files = 0u64;

    for path in &files {
        let rel = path
            .strip_prefix(dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('/', "_");
        let (chunks, bytes, skipped) = backup_file(path, store_dir, &timestamp, &rel);
        total_chunks += chunks;
        total_bytes += bytes;
        total_skipped += skipped;
        total_files += 1;
    }
    println!("Directory backup complete!");
    println!("Files: {total_files}");
    println!("Chunks: {total_chunks}");
    println!("Total bytes: {total_bytes}");
    println!("Dedup skipped (already stored): {total_skipped}");
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

pub fn backup_file(source: &Path, store_dir: &Path, timestamp: &str, label: &str) -> (u64, u64, u64) {
    println!("Backup: {}", source.display());
    let file_source = match FileSource::new(source) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error opening file: {}", e);
            return (0, 0, 0);
        }
    };
    let metadata = file_source.metadata();
    println!("File size: {} bytes", metadata.size);
    let chunks_dir = store_dir.join("chunks");
    let _ = fs::create_dir_all(&chunks_dir);
    let local_store = match LocalChunkStore::new(chunks_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error creating chunk store: {}", e);
            return (0, 0, 0);
        }
    };
    let mut chunk_store = CompressedChunkStore::new(local_store);
    let index_path = store_dir.join("index.bincode");
    let mut dedup_index = match PersistentDedupIndex::new(index_path) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: failed to load dedup index: {e}");
            return (0, 0, 0);
        }
    };
    let chunker = StreamCDC::new(file_source, 4096, 16384, 65536);
    let mut chunk_hashes: Vec<[u8; 32]> = Vec::new();
    let mut chunk_count = 0;
    let mut total_bytes = 0;
    let mut dedup_skipped = 0u64;
    let pb = ProgressBar::new_spinner();

    pb.set_style(
        ProgressStyle::with_template("{spinner} Chunks: {pos} | Bytes: {bytes} | Dedup: {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈"),
    );

    for chunk_result in chunker {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                pb.finish_and_clear();
                eprintln!("Error reading chunk: {e}");
                return (0, 0, 0);
            }
        };
        let data = &chunk.data;
        let hash = hash_data(data);

        if let Ok(Some(_entry)) = dedup_index.lookup(&hash) {
            let _ = dedup_index.increment_refcount(&hash);
            dedup_skipped += 1;
        } else {
            let uncompressed = data.len() as u32;
            let location = match chunk_store.put(&hash, data) {
                Ok(loc) => loc,
                Err(e) => {
                    pb.finish_and_clear();
                    eprintln!("Error storing chunk: {e}");
                    return (0, 0, 0);
                }
            };
            let _ = dedup_index.insert(&hash, location, uncompressed, uncompressed);
        }

        chunk_hashes.push(hash);
        chunk_count += 1;
        total_bytes += data.len() as u64;
        pb.inc(1);
        pb.set_message(format!("{dedup_skipped}"));
        pb.set_position(chunk_count);
    }
    pb.finish_and_clear();

    if let Err(e) = dedup_index.save() {
        eprintln!("Warning: failed to save dedup index: {e}");
    }

    let manifest = Manifest {
        version: rmrf_manifest::MANIFEST_VERSION,
        source: metadata,
        chunks: chunk_hashes,
    };
    let manifests_dir = store_dir.join("manifests");
    let _ = fs::create_dir_all(&manifests_dir);
    let manifest_path = manifests_dir.join(format!("{timestamp}-{label}.bincode"));

    match manifest.save(&manifest_path) {
        Ok(_) => println!("\nManifest saved: {}", manifest_path.display()),
        Err(e) => eprintln!("Error saving manifest: {}", e),
    }
    println!("Backup complete!");
    println!("Chunks: {}", chunk_count);
    println!("Total bytes: {}", total_bytes);
    println!("Dedup skipped (already stored): {}", dedup_skipped);

    (chunk_count, total_bytes, dedup_skipped)
}

pub fn backup_vm(
    path: &std::path::Path,
    store_dir: &std::path::Path,
    vm_id: &str,
    disk_name: &str,
    format: DiskFormat,
    lifecycle: Box<dyn VmLifecycle>,
) {
    let vm_source = match VmSource::open(path, vm_id, disk_name, format, lifecycle) {
        Ok(vs) => vs,
        Err(e) => {
            eprintln!("Error opening VM source: {e}");
            return;
        }
    };
    let metadata = vm_source.metadata();

    let chunks_dir = store_dir.join("chunks");
    let _ = fs::create_dir_all(&chunks_dir);
    let local_store = match LocalChunkStore::new(chunks_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error creating chunk store: {}", e);
            return;
        }
    };
    let mut chunk_store = CompressedChunkStore::new(local_store);
    let index_path = store_dir.join("index.bincode");
    let mut dedup_index = match PersistentDedupIndex::new(index_path) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: failed to load dedup index: {e}");
            return;
        }
    };

    let chunker = StreamCDC::new(vm_source, 4096, 16384, 65536);
    let mut chunk_hashes: Vec<[u8; 32]> = Vec::new();
    let mut chunk_count = 0;
    let mut total_bytes = 0;
    let mut dedup_skipped = 0u64;

    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner} Chunks: {pos} | Bytes: {bytes} | Dedup: {msg}")
            .unwrap_or_else(|_| ProgressStyle::default_spinner())
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈"),
    );

    for chunk_result in chunker {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                pb.finish_and_clear();
                eprintln!("Error reading chunk: {e}");
                return;
            }
        };
        let data = &chunk.data;
        let hash = hash_data(data);

        if let Ok(Some(_entry)) = dedup_index.lookup(&hash) {
            let _ = dedup_index.increment_refcount(&hash);
            dedup_skipped += 1;
        } else {
            let uncompressed = data.len() as u32;
            let location = match chunk_store.put(&hash, data) {
                Ok(loc) => loc,
                Err(e) => {
                    pb.finish_and_clear();
                    eprintln!("Error storing chunk: {e}");
                    return;
                }
            };
            let _ = dedup_index.insert(&hash, location, uncompressed, uncompressed);
        }

        chunk_hashes.push(hash);
        chunk_count += 1;
        total_bytes += data.len() as u64;
        pb.inc(1);
        pb.set_message(format!("{dedup_skipped}"));
        pb.set_position(chunk_count);
    }
    pb.finish_and_clear();

    let manifests_dir = store_dir.join("manifests");
    let _ = fs::create_dir_all(&manifests_dir);

    if let Err(e) = dedup_index.save() {
        eprintln!("Warning: failed to save dedup index: {e}");
    }

    let manifest = Manifest {
        version: rmrf_manifest::MANIFEST_VERSION,
        source: metadata,
        chunks: chunk_hashes,
    };
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let basename = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
    let manifest_path = manifests_dir.join(format!("{timestamp}-{basename}.bincode"));

    match manifest.save(&manifest_path) {
        Ok(_) => println!("\nManifest saved: {}", manifest_path.display()),
        Err(e) => eprintln!("Error saving manifest: {e}"),
    }
    println!("Backup complete!");
    println!("Chunks: {}", chunk_count);
    println!("Total bytes: {}", total_bytes);
    println!("Dedup skipped (already stored): {}", dedup_skipped);
}
