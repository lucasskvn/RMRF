use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rmrf_core::{ChunkStore, Hash};
use rmrf_manifest::Manifest;
use rmrf_store::{hash_data, CompressedChunkStore, EncryptedChunkStore, LocalChunkStore};
use std::fs::{self, FileTimes};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime};

const RESTORE_BATCH: usize = 64;

struct RestoreCtx {
    manifest: Manifest,
    output_path: PathBuf,
    chunk_store: Box<dyn ChunkStore>,
    jobs: usize,
}

fn prepare_restore(
    manifest_path: &Path,
    store_dir: &Path,
    output: &Path,
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
    let output_path = output.to_path_buf();
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
    Some(RestoreCtx {
        manifest,
        output_path,
        chunk_store,
        jobs,
    })
}

pub fn restore(
    manifest_path: &Path,
    store_dir: &Path,
    output: &Path,
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
        apply_file_metadata(output_path, manifest).ok();
        println!("Restored empty file: {}", output_path.display());
        true
    } else {
        false
    }
}

fn check_overwrite(path: &Path, force: bool) -> bool {
    if !path.exists() {
        return true;
    }
    if force {
        return true;
    }
    if path.is_dir() || path.is_symlink() {
        eprintln!(
            "Error: output path is a directory or symlink: {}",
            path.display()
        );
        eprintln!("  (use --force to overwrite)");
        return false;
    }
    eprintln!(
        "Error: output file already exists: {} (use --force)",
        path.display()
    );
    false
}

pub(crate) fn open_chunk_store(
    store_dir: &Path,
    manifest: &Manifest,
    identity: Option<&str>,
) -> Result<Box<dyn ChunkStore>, String> {
    let chunks_dir = store_dir.join("chunks");
    let local_store = LocalChunkStore::new(chunks_dir).map_err(|e| e.to_string())?;
    let compressed = CompressedChunkStore::new(local_store);
    if manifest.encryption.is_some() {
        let identity_path = identity
            .ok_or_else(|| "Error: manifest is encrypted but no --identity provided".to_string())?;
        let identity_str = std::fs::read_to_string(identity_path)
            .map_err(|e| format!("Error reading identity file: {e}"))?;
        let identity = age::x25519::Identity::from_str(identity_str.trim())
            .map_err(|e| format!("Invalid age identity: {e}"))?;
        let recipient = identity.to_public();
        Ok(Box::new(EncryptedChunkStore::new(
            compressed, recipient, identity,
        )))
    } else {
        Ok(Box::new(compressed))
    }
}

pub fn restore_file(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    dest: &Path,
    jobs: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()?;
    let tmp = tmp_path(dest);
    let mut output = fs::File::create(&tmp)?;
    let total = manifest.chunks.len();
    let pb = create_progress_bar(total as u64);

    for batch in manifest.chunks.chunks(RESTORE_BATCH) {
        let results: Vec<Result<Vec<u8>, String>> = pool.install(|| {
            batch
                .par_iter()
                .map(|hash| fetch_chunk(store, hash))
                .collect()
        });
        for res in results {
            match res {
                Ok(data) => {
                    output.write_all(&data)?;
                    pb.inc(1);
                }
                Err(e) => {
                    pb.finish_and_clear();
                    let _ = fs::remove_file(&tmp);
                    eprintln!("Error: {e}");
                    return Err(e.into());
                }
            }
        }
    }
    output.sync_all()?;
    apply_file_handle_metadata(&output, manifest)?;
    drop(output);
    fs::rename(&tmp, dest)?;
    pb.finish_and_clear();

    println!("Total chunks restored: {total}");
    println!("Total size: {} bytes", manifest.source.size);
    Ok(())
}

fn fetch_chunk(store: &dyn ChunkStore, hash: &Hash) -> Result<Vec<u8>, String> {
    match store.get(hash) {
        Ok(Some(data)) => {
            if &hash_data(&data) != hash {
                return Err(format!(
                    "chunk {:02x?}... corrupted (hash mismatch)",
                    &hash[..4]
                ));
            }
            Ok(data)
        }
        Ok(None) => Err(format!("chunk {:02x?}... not found in store", &hash[..4])),
        Err(e) => Err(format!("chunk {:02x?}... read error: {e}", &hash[..4])),
    }
}

fn tmp_path(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.tmp", dest.display()))
}

fn apply_file_handle_metadata(
    file: &fs::File,
    manifest: &Manifest,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        let mode = manifest.source.permissions;
        if mode != 0 {
            file.set_permissions(fs::Permissions::from_mode(mode)).ok();
        }
    }
    if manifest.source.modified > 0 {
        let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(manifest.source.modified as u64);
        file.set_times(FileTimes::new().set_modified(mtime)).ok();
    }
    Ok(())
}

fn apply_file_metadata(path: &Path, manifest: &Manifest) -> Result<(), Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    apply_file_handle_metadata(&file, manifest)
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
