use clap::{Parser, Subcommand, ValueEnum};
use fastcdc::v2016::StreamCDC;
use rmrf_core::{ChunkStore, DedupIndex, DiskFormat, Source};
use rmrf_manifest::Manifest;
use rmrf_source::{FileSource, LibvirtLifecycle, NoopLifecycle, ProxmoxLifecycle, VmLifecycle, VmSource};
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore, PersistentDedupIndex};
use std::fmt;
use std::fs;
use std::collections::HashSet;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};

const DEFAULT_STORE: &str = "./rmrf-store";

#[derive(ValueEnum, Clone)]
enum Hypervisor {
    Auto,
    Proxmox,
    Libvirt,
    Noop,
}

impl fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hypervisor::Auto => write!(f, "auto"),
            Hypervisor::Proxmox => write!(f, "proxmox"),
            Hypervisor::Libvirt => write!(f, "libvirt"),
            Hypervisor::Noop => write!(f, "noop"),
        }
    }
}

#[derive(ValueEnum, Clone)]
enum DiskFormatArg {
    Auto,
    Raw,
    Qcow2,
}

#[derive(Parser)]
#[command(name = "rmrf", about = "RMRF backup tool — chunk-based deduplicated backups")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a backup of a file or VM disk
    Backup {
        /// Path to the file or VM disk to backup
        path: PathBuf,
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        /// Delete manifests older than N days after backup, then run GC
        #[arg(long)]
        retain: Option<u64>,
        /// VM ID for Proxmox/Libvirt VM backup (suspends VM, reads disk, resumes VM)
        #[arg(long)]
        vm: Option<String>,
        /// Disk name for VM metadata (default: basename of path)
        #[arg(long)]
        disk_name: Option<String>,
        /// Disk format: auto (detect from extension), raw, or qcow2
        #[arg(long, default_value = "auto")]
        format: DiskFormatArg,
        /// Skip VM suspend/resume (useful for already-snapshotted disks)
        #[arg(long)]
        no_suspend: bool,
        /// Hypervisor type for VM backup
        #[arg(long, default_value = "auto")]
        hypervisor: Hypervisor,
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
    /// List all backups in the store
    List {
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Show detailed info for a specific backup
    Info {
        /// Path to the manifest file (.bincode)
        manifest: PathBuf,
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Verify a backup (check all chunks exist on disk)
    Verify {
        /// Path to the manifest file (.bincode)
        manifest: PathBuf,
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Garbage collect orphan chunks
    Gc {
        /// Store directory (default: ./rmrf-store)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        /// Dry run: report what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
    },
    /// Clean test artifacts from the current directory
    Clean,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Backup {
            path,
            store,
            retain,
            vm,
            disk_name,
            format,
            no_suspend,
            hypervisor,
        } => {
            let format = parse_disk_format(&format, &path);
            let lifecycle = if no_suspend {
                Box::new(NoopLifecycle) as Box<dyn VmLifecycle>
            } else {
                build_vm_lifecycle(vm.as_deref(), &hypervisor)
            };

            if let Some(vm_id) = vm {
                let disk = disk_name.as_deref().unwrap_or_else(|| {
                    path.file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("disk")
                });
                backup_vm(&path, &store, &vm_id, disk, format, lifecycle);
            } else {
                backup(&path, &store);
            }

            if let Some(days) = retain {
                run_retention_and_gc(&store, days);
            }
        }
        Command::Restore {
            manifest,
            store,
            output,
        } => {
            restore(&manifest, &store, output.as_deref());
        }
        Command::List { store } => list(&store),
        Command::Info { manifest, store } => info(&manifest, &store),
        Command::Verify { manifest, store } => verify(&manifest, &store),
        Command::Gc { store, dry_run } => gc(&store, dry_run),
        Command::Clean => clean(),
    }
}

fn backup(source: &Path, store_dir: &Path) {
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

    println!();
    println!("Directory backup complete!");
    println!("Files: {total_files}");
    println!("Chunks: {total_chunks}");
    println!("Total bytes: {total_bytes}");
    println!("Dedup skipped (already stored): {total_skipped}");
}

fn walk_files(dir: &Path) -> Result<Vec<PathBuf>, io::Error> {
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

fn backup_file(source: &Path, store_dir: &Path, timestamp: &str, label: &str) -> (u64, u64, u64) {
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

    for chunk_result in chunker {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error reading chunk: {}", e);
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
                    eprintln!("Error storing chunk: {}", e);
                    return (0, 0, 0);
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

    if let Err(e) = dedup_index.save() {
        eprintln!("Warning: failed to save dedup index: {e}");
    }

    let manifest = Manifest {
        version: rmrf_manifest::MANIFEST_VERSION,
        source: metadata,
        chunks: chunk_hashes,
    };
    let basename = label;
    let manifests_dir = store_dir.join("manifests");
    let _ = fs::create_dir_all(&manifests_dir);
    let manifest_path = manifests_dir.join(format!("{timestamp}-{basename}.bincode"));

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

fn parse_disk_format(arg: &DiskFormatArg, path: &Path) -> DiskFormat {
    match arg {
        DiskFormatArg::Auto => {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("qcow2") {
                    return DiskFormat::Qcow2;
                }
            }
            DiskFormat::Raw
        }
        DiskFormatArg::Raw => DiskFormat::Raw,
        DiskFormatArg::Qcow2 => DiskFormat::Qcow2,
    }
}

fn detect_hypervisor(vm_id: &str) -> Hypervisor {
    if std::process::Command::new("qm")
        .args(["status", vm_id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Hypervisor::Proxmox;
    }
    if std::process::Command::new("virsh")
        .args(["dominfo", vm_id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Hypervisor::Libvirt;
    }
    Hypervisor::Noop
}

fn build_vm_lifecycle(vm_id: Option<&str>, hypervisor: &Hypervisor) -> Box<dyn VmLifecycle> {
    let hv = match hypervisor {
        Hypervisor::Auto => {
            if let Some(id) = vm_id {
                detect_hypervisor(id)
            } else {
                Hypervisor::Noop
            }
        }
        Hypervisor::Proxmox => Hypervisor::Proxmox,
        Hypervisor::Libvirt => Hypervisor::Libvirt,
        Hypervisor::Noop => Hypervisor::Noop,
    };
    match hv {
        Hypervisor::Proxmox => Box::new(ProxmoxLifecycle),
        Hypervisor::Libvirt => Box::new(LibvirtLifecycle),
        _ => Box::new(NoopLifecycle),
    }
}

fn backup_vm(
    path: &Path,
    store_dir: &Path,
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
            eprintln!("Error creating chunk store: {e}");
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

    for chunk_result in chunker {
        let chunk = match chunk_result {
            Ok(c) => c,
            Err(e) => {
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
                    eprintln!("Error storing chunk: {e}");
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
    let basename = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
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

fn list(store_dir: &Path) {
    let manifests_dir = store_dir.join("manifests");
    let mut entries: Vec<_> = match fs::read_dir(&manifests_dir) {
        Ok(entries) => entries.filter_map(|e| e.ok()).collect(),
        Err(e) => {
            eprintln!("Error reading manifests directory: {e}");
            return;
        }
    };
    entries.sort_by_key(|e| e.file_name());

    if entries.is_empty() {
        println!("No backups found.");
        return;
    }

    for entry in &entries {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "bincode") {
            continue;
        }
        let manifest = match Manifest::load(&path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("Warning: could not load {}: {}", path.display(), e);
                continue;
            }
        };
        let basename = path.file_name().unwrap().to_string_lossy();
        let timestamp = basename.split('-').next().unwrap_or("unknown");
        println!("{}  {:>9}  {:>4} chunks  {}",
            timestamp,
            format_size(manifest.source.size),
            manifest.chunks.len(),
            manifest.source.path,
        );
    }
}

fn info(manifest_path: &Path, _store_dir: &Path) {
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return;
        }
    };
    println!("Version:     v{}", manifest.version);
    println!("Source path: {}", manifest.source.path);
    println!("Source size: {} ({})", manifest.source.size, format_size(manifest.source.size));
    println!("Source type: {:?}", manifest.source.source_type);
    println!("Modified:    {}", manifest.source.modified);
    println!("Permissions: {:o}", manifest.source.permissions);
    println!("Chunks:      {}", manifest.chunks.len());
}

fn verify(manifest_path: &Path, store_dir: &Path) {
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

fn gc(store_dir: &Path, dry_run: bool) {
    let manifests_dir = store_dir.join("manifests");
    let mut referenced: HashSet<[u8; 32]> = HashSet::new();

    // Collect all hashes from all manifests
    if let Ok(entries) = fs::read_dir(&manifests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "bincode") {
                continue;
            }
            if let Ok(m) = Manifest::load(&path) {
                for hash in m.chunks {
                    referenced.insert(hash);
                }
            }
        }
    }

    let chunks_dir = store_dir.join("chunks");
    let hex_char = |b: u8| b.is_ascii_hexdigit();
    let mut orphans = 0u64;
    let mut freed_bytes = 0u64;

    if let Ok(entries) = fs::read_dir(&chunks_dir) {
        for prefix in entries.flatten() {
            let name = prefix.file_name().to_string_lossy().to_string();
            if name.len() != 2 || !name.bytes().all(hex_char) {
                continue;
            }
            if let Ok(inner_entries) = fs::read_dir(prefix.path()) {
                for inner in inner_entries.flatten() {
                    let inner_name = inner.file_name().to_string_lossy().to_string();
                    let hex_str = format!("{}{}", name, inner_name);
                    if let Some(hash_bytes) = hex_to_bytes(&hex_str) {
                        if hash_bytes.len() == 32 {
                            let mut hash = [0u8; 32];
                            hash.copy_from_slice(&hash_bytes);
                            if !referenced.contains(&hash) {
                                let size = inner.metadata().map(|m| m.len()).unwrap_or(0);
                                if dry_run {
                                    println!("Would delete orphan: {}/{inner_name} ({})", name, format_size(size));
                                } else {
                                    fs::remove_file(inner.path()).ok();
                                    println!("Deleted orphan: {}/{inner_name} ({})", name, format_size(size));
                                }
                                orphans += 1;
                                freed_bytes += size;
                            }
                        }
                    }
                }
            }
            // Cleanup empty prefix dirs
            if !dry_run
                && prefix.path().read_dir().map_or(true, |mut d| d.next().is_none()) {
                    fs::remove_dir(prefix.path()).ok();
                }
        }
    }

    if dry_run {
        println!("Dry run: would delete {orphans} orphan chunks, would free {}", format_size(freed_bytes));
    } else {
        println!("Deleted {orphans} orphan chunks, freed {}", format_size(freed_bytes));
    }
}

fn run_retention_and_gc(store_dir: &Path, days: u64) {
    let manifests_dir = store_dir.join("manifests");
    let now = chrono::Utc::now().timestamp();
    let cutoff = now - (days as i64) * 86400;

    let index_path = store_dir.join("index.bincode");
    let mut dedup_index = match PersistentDedupIndex::new(index_path) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Warning: failed to load dedup index for retention: {e}");
            return;
        }
    };

    let mut deleted_manifests = 0u64;
    if let Ok(entries) = fs::read_dir(&manifests_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "bincode") {
                continue;
            }
            let basename = path.file_name().unwrap().to_string_lossy();
            let ts_str = basename.split('-').next().unwrap_or("");
            let ts = parse_timestamp(ts_str);
            if ts < cutoff {
                if let Ok(m) = Manifest::load(&path) {
                    for hash in &m.chunks {
                        let _ = dedup_index.decrement_refcount(hash);
                    }
                    fs::remove_file(&path).ok();
                    deleted_manifests += 1;
                    println!("Removed manifest: {}", path.display());
                }
            }
        }
    }

    if deleted_manifests > 0 {
        let _ = dedup_index.save();
        println!("Retention: removed {deleted_manifests} manifest(s) older than {days} day(s)");
        gc(store_dir, false);
    } else {
        println!("Retention: no manifests older than {days} day(s)");
    }
}

fn parse_timestamp(s: &str) -> i64 {
    if s.len() < 15 {
        return 0;
    }
    let year: i32 = s[..4].parse().unwrap_or(1970);
    let month: u32 = s[4..6].parse().unwrap_or(1);
    let day: u32 = s[6..8].parse().unwrap_or(1);
    let hour: u32 = s[9..11].parse().unwrap_or(0);
    let min: u32 = s[11..13].parse().unwrap_or(0);
    let sec: u32 = s[13..15].parse().unwrap_or(0);
    let days = days_from_civil(year, month, day) - days_from_civil(1970, 1, 1);
    days as i64 * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = y - (m <= 2) as i32;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (m + if m > 2 { 3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1_073_741_824 {
        format!("{:.1} GiB", bytes as f64 / 1_073_741_824.0)
    } else if bytes >= 1_048_576 {
        format!("{:.1} MiB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
}

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<Result<Vec<_>, _>>()
        .ok()
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
