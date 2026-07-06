use clap::{Parser, Subcommand};
use fastcdc::v2016::StreamCDC;
use rmrf_core::{ChunkStore, DedupIndex, Source};
use rmrf_manifest::Manifest;
use rmrf_source::FileSource;
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore, PersistentDedupIndex};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const DEFAULT_STORE: &str = "./rmrf-store";

#[derive(Parser)]
#[command(name = "rmrf", about = "RMRF backup tool — chunk-based deduplicated backups")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a backup of a file
    Backup {
        /// Path to the file to backup
        path: PathBuf,
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Restore a file from a backup manifest
    Restore {
        /// Path to the manifest file (.bincode)
        manifest: PathBuf,
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        /// Output path for the restored file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Clean test artifacts from the current directory
    Clean,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Backup { path, store } => {
            backup(&path, &store);
        }
        Command::Restore {
            manifest,
            store,
            output,
        } => {
            restore(&manifest, &store, output.as_deref());
        }
        Command::Clean => {
            clean();
        }
    }
}

fn backup(source: &Path, store_dir: &Path) {
    println!("Backup: {}", source.display());

    let file_source = match FileSource::new(source) {
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
    let index_path = store_dir.join("index.bincode");
    let mut dedup_index = match PersistentDedupIndex::new(index_path) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: failed to load dedup index: {e}");
            return;
        }
    };

    let chunker = StreamCDC::new(file_source, 4096, 16384, 65536);
    let mut chunk_hashes: Vec<[u8; 32]> = Vec::new();
    let mut chunk_count = 0;
    let mut total_bytes = 0;
    let mut dedup_skipped = 0u64;

    for chunk_result in chunker {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading chunk: {}", e);
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
                    eprintln!("Error storing chunk: {}", e);
                    return;
                }
            };
            let _ = dedup_index.insert(&hash, location, uncompressed, uncompressed);
        }

        chunk_hashes.push(hash);
        chunk_count += 1;
        total_bytes += data.len() as u64;
        print!(".");
        std::io::Write::flush(&mut std::io::stdout()).ok();
    }

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

fn restore(manifest_path: &Path, store_dir: &Path, output: Option<&Path>) {
    println!("Restore: {}", manifest_path.display());

    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {}", e);
            return;
        }
    };

    let output_path = output.unwrap_or_else(|| {
        let basename = manifest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("restored");
        let name = basename
            .strip_suffix(".bincode")
            .unwrap_or(basename);
        let parts: Vec<&str> = name.splitn(2, '-').collect();
        Path::new(parts.last().copied().unwrap_or(parts[0]))
    });

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
        fs::write(output_path, b"").ok();
        println!("Restored empty file: {}", output_path.display());
        return;
    }

    match restore_file(&manifest, &chunk_store, output_path) {
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
        "\u{e9}co\u{df}\u{6d4b}\u{8bd5}.txt",
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
