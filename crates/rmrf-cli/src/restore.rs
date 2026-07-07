use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rmrf_core::ChunkStore;
use rmrf_manifest::Manifest;
use rmrf_store::{CompressedChunkStore, EncryptedChunkStore, LocalChunkStore};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::str::FromStr;

const RESTORE_BATCH: usize = 64;

struct RestoreCtx {
    manifest: Manifest,
    output_path: std::path::PathBuf,
    chunk_store: Box<dyn ChunkStore>,
    jobs: usize,
}

fn prepare_restore(
    manifest_path: &Path,
    store_dir: &Path,
    output: Option<&Path>,
    force: bool,
    jobs: usize,
    identity: Option<&str>,
) -> Option<RestoreCtx> {
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return None;
        }
    };
    let output_path = resolve_output_path_or_default(output, manifest_path);
    if !check_overwrite(&output_path, force) {
        return None;
    }
    let chunk_store = match open_chunk_store(store_dir, &manifest, identity) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };
    Some(RestoreCtx { manifest, output_path, chunk_store, jobs })
}

pub fn restore(
    manifest_path: &Path,
    store_dir: &Path,
    output: Option<&Path>,
    force: bool,
    jobs: usize,
    identity: Option<&str>,
) -> bool {
    println!("Restore: {}", manifest_path.display());

    let ctx = match prepare_restore(manifest_path, store_dir, output, force, jobs, identity) {
        Some(c) => c,
        None => return false,
    };

    if is_empty_manifest(&ctx.manifest, &ctx.output_path) {
        return true;
    }

    match restore_file(&ctx.manifest, &*ctx.chunk_store, &ctx.output_path, ctx.jobs) {
        Ok(_) => {
            println!("Restore complete: {}", ctx.output_path.display());
            true
        }
        Err(e) => {
            eprintln!("Restore failed: {e}");
            false
        }
    }
}

fn is_empty_manifest(manifest: &Manifest, output_path: &Path) -> bool {
    if manifest.chunks.is_empty() {
        if let Err(e) = fs::write(output_path, b"") {
            eprintln!("Error writing empty file: {e}");
            return false;
        }
        if let Ok(f) = fs::File::open(output_path) {
            f.sync_all().ok();
        }
        println!("Restored empty file: {}", output_path.display());
        true
    } else {
        false
    }
}

fn resolve_output_path_or_default(
    output: Option<&Path>,
    manifest_path: &Path,
) -> std::path::PathBuf {
    output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| resolve_output_path(manifest_path))
}

fn check_overwrite(path: &Path, force: bool) -> bool {
    if !path.exists() {
        return true;
    }
    if force {
        return true;
    }
    if path.is_dir() || path.is_symlink() {
        eprintln!("Error: output path is a directory or symlink: {}",
            path.display());
        eprintln!("  (use --force to overwrite)");
        return false;
    }
    eprintln!("Error: output file already exists: {} (use --force)",
        path.display());
    false
}

fn open_chunk_store(
    store_dir: &Path,
    manifest: &Manifest,
    identity: Option<&str>,
) -> Result<Box<dyn ChunkStore>, String> {
    let chunks_dir = store_dir.join("chunks");
    let local_store = LocalChunkStore::new(chunks_dir).map_err(|e| e.to_string())?;
    let compressed = CompressedChunkStore::new(local_store);
    if manifest.encryption.is_some() {
        let identity_path = identity.ok_or_else(|| {
            "Error: manifest is encrypted but no --identity provided".to_string()
        })?;
        let identity_str = std::fs::read_to_string(identity_path)
            .map_err(|e| format!("Error reading identity file: {e}"))?;
        let identity_str = identity_str.trim();
        let identity = age::x25519::Identity::from_str(identity_str)
            .map_err(|e| format!("Invalid age identity: {e}"))?;
        let recipient = identity.to_public();
        Ok(Box::new(EncryptedChunkStore::new(compressed, recipient, identity)))
    } else {
        Ok(Box::new(compressed))
    }
}

fn resolve_output_path(manifest_path: &Path) -> std::path::PathBuf {
    let basename = manifest_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("restored");
    let name = basename.strip_suffix(".bincode").unwrap_or(basename);
    let parts: Vec<&str> = name.splitn(2, '-').collect();
    parts
        .last()
        .copied()
        .unwrap_or(parts[0])
        .into()
}

pub fn restore_file(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    dest: &Path,
    jobs: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build()?;
    let mut output = fs::File::create(dest)?;
    let total = manifest.chunks.len();

    let pb = create_progress_bar(total as u64);

    for batch in manifest.chunks.chunks(RESTORE_BATCH) {
        let results: Vec<Option<Vec<u8>>> = pool.install(|| {
            batch.par_iter()
                .map(|hash| store.get(hash).unwrap_or(None))
                .collect()
        });
        for (hash, data) in batch.iter().zip(results) {
            match data {
                Some(d) => {
                    output.write_all(&d)?;
                    pb.inc(1);
                }
                None => {
                    pb.finish_and_clear();
                    eprintln!("Error: chunk {:?} not found in store", &hash[..4]);
                    return Err("chunk not found".into());
                }
            }
        }
    }
    output.sync_all()?;
    #[cfg(unix)]
    {
        let mode = manifest.source.permissions;
        if mode != 0 {
            output.set_permissions(fs::Permissions::from_mode(mode)).ok();
        }
    }
    drop(output);
    pb.finish_and_clear();

    println!("Total chunks restored: {total}");
    println!("Total size: {} bytes", manifest.source.size);

    Ok(())
}

fn create_progress_bar(total: u64) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template("{bar:30.cyan/blue} {pos}/{len} chunks")
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=> "),
    );
    pb
}
