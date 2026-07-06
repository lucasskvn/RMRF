use rmrf_core::{ChunkStore, DedupIndex, Source};
use rmrf_manifest::Manifest;
use rmrf_source::FileSource;
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore, MemoryDedupIndex};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const CHUNK_SIZE: usize = 8192;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Error: no command");
        eprintln!("Usage: rmrf <backup|restore|clean> [args...]");
        std::process::exit(1);
    }
    let cmd = &args[1];

    if cmd != "backup" && cmd != "restore" && cmd != "clean" {
        eprintln!("Unknown command: {cmd}");
        eprintln!("Usage: rmrf <backup|restore|clean> [args...]");
        std::process::exit(1);
    }

    if cmd == "clean" {
        clean();
        return;
    }

    if args.len() < 3 {
        eprintln!("Error: missing path argument for '{cmd}'");
        eprintln!("Usage: rmrf {cmd} <path> [store_dir]");
        std::process::exit(1);
    }
    let path = PathBuf::from(&args[2]);
    let store_dir = args
        .get(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("./rmrf-store"));

    match cmd.as_str() {
        "backup" => backup(&path, &store_dir),
        "restore" => restore(&path, &store_dir),
        _ => unreachable!(),
    }
}

fn backup(source: &Path, store_dir: &Path) {
    println!("Backup: {}", source.display());

    let mut file_source = match FileSource::new(source) {
        Ok(fs) => fs,
        Err(e) => {
            eprintln!("Error opening file: {}", e);
            return;
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
            return;
        }
    };
    let mut chunk_store = CompressedChunkStore::new(local_store);
    let mut dedup_index = MemoryDedupIndex::new();

    let mut buffer = vec![0u8; CHUNK_SIZE];
    let mut chunk_hashes: Vec<[u8; 32]> = Vec::new();
    let mut chunk_count = 0;
    let mut total_bytes = 0;
    let mut dedup_skipped = 0u64;

    loop {
        match file_source.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let data = &buffer[..n];
                let hash = hash_data(data);

                if let Ok(Some(_entry)) = dedup_index.lookup(&hash) {
                    let _ = dedup_index.increment_refcount(&hash);
                    dedup_skipped += 1;
                } else {
                    let uncompressed = data.len() as u32;
                    let location = match chunk_store.put(&hash, data) {
                        Ok(loc) => loc,
                        Err(e) => {
                            eprintln!("Error storing chunk: {}", e);
                            return;
                        }
                    };
                    let _ = dedup_index.insert(&hash, location, uncompressed, uncompressed);
                }

                chunk_hashes.push(hash);
                chunk_count += 1;
                total_bytes += n as u64;
                print!(".");
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            Err(e) => {
                eprintln!("Error reading file: {}", e);
                return;
            }
        }
    }

    let manifests_dir = store_dir.join("manifests");
    let _ = fs::create_dir_all(&manifests_dir);
    let manifest = Manifest {
        version: 1,
        source: metadata,
        chunks: chunk_hashes,
    };
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let basename = source
        .file_name()
        .unwrap_or(source.as_os_str())
        .to_string_lossy();
    let manifest_path = manifests_dir.join(format!("{timestamp}-{basename}.bincode"));

    match manifest.save(&manifest_path) {
        Ok(_) => println!("\nManifest saved: {}", manifest_path.display()),
        Err(e) => eprintln!("Error saving manifest: {}", e),
    }
    println!("Backup complete!");
    println!("Chunks: {}", chunk_count);
    println!("Total bytes: {}", total_bytes);
    println!("Dedup skipped (already stored): {}", dedup_skipped);
}

fn restore(dest: &Path, store_dir: &Path) {
    println!("Restore: {}", dest.display());
    let manifests_dir = store_dir.join("manifests");
    let mut latest_manifest: Option<PathBuf> = None;

    if let Ok(entries) = fs::read_dir(&manifests_dir) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_file() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "bincode") {
                        latest_manifest = Some(path);
                        break;
                    }
                }
            }
        }
    }

    let manifest_path = match latest_manifest {
        Some(p) => p,
        None => {
            eprintln!("Error: no manifest found in store");
            return;
        }
    };

    let manifest = match Manifest::load(&manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {}", e);
            return;
        }
    };

    let chunks_dir = store_dir.join("chunks");
    let local_store = match LocalChunkStore::new(chunks_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error opening chunk store: {}", e);
            return;
        }
    };
    let chunk_store = CompressedChunkStore::new(local_store);

    if manifest.chunks.is_empty() {
        fs::write(dest, b"").ok();
        println!("Restored empty file: {}", dest.display());
        return;
    }

    let output_path = dest.to_path_buf();

    match restore_file(&manifest, &chunk_store, &output_path) {
        Ok(_) => println!("Restore complete: {}", output_path.display()),
        Err(e) => eprintln!("Restore failed: {}", e),
    }
}

fn restore_file(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    dest: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = fs::File::create(dest)?;
    let total = manifest.chunks.len();

    for (i, hash) in manifest.chunks.iter().enumerate() {
        match store.get(hash)? {
            Some(data) => {
                output.write_all(&data)?;
                if total > 10 && i % (total / 10) == 0 {
                    print!("\rProgress: {}%", (i * 100) / total);
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
            }
            None => {
                eprintln!("\nError: chunk {:?} not found in store", &hash[..4]);
                return Err("chunk not found".into());
            }
        }
    }

    if total > 10 {
        println!("\rProgress: 100%");
    }
    println!("Total chunks restored: {total}");
    println!("Total size: {} bytes", manifest.source.size);

    Ok(())
}

fn clean() {
    let artifacts = [
        "empty",
        "one_byte",
        "zeros",
        "ff_bytes",
        "repeated",
        "exact_chunk",
        "chunk_plus",
        "restored",
        "rmrf_src",
        "test.txt",
        "input.txt",
        "input.bin",
        "rmrf_input.txt",
        "-rmrf_input.txt",
        "écoß测试.txt",
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
