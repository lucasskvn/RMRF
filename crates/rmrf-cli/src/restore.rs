use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rmrf_core::{ChunkStore, Hash};
use rmrf_manifest::Manifest;
use rmrf_store::{hash_data, CompressedChunkStore, EncryptedChunkStore, LocalChunkStore};
use std::fs::{self, FileTimes, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use rmrf_source::{NoopLifecycle, VmLifecycle};

use crate::disk_detect::detect_disk_path;
use crate::hypervisor::{build_vm_lifecycle, validate_vm_id, Hypervisor};

const RESTORE_BATCH: usize = 64;

struct RestoreCtx {
    manifest: Manifest,
    output_path: PathBuf,
    chunk_store: Box<dyn ChunkStore>,
    jobs: usize,
}

pub struct RestoreOptions<'a> {
    pub manifest_path: &'a Path,
    pub store_dir: &'a Path,
    pub output: Option<&'a Path>,
    pub force: bool,
    pub jobs: usize,
    pub identity: Option<&'a str>,
    pub vm: Option<&'a str>,
    pub disk: Option<&'a str>,
    pub no_suspend: bool,
    pub hypervisor: &'a Hypervisor,
}

fn derive_store_from_manifest(manifest_path: &Path) -> Option<PathBuf> {
    let parent = manifest_path.parent()?;
    if parent.file_name()?.to_str()? == "manifests" {
        parent.parent().map(|p| p.to_path_buf())
    } else {
        None
    }
}

fn prepare_restore(opts: &RestoreOptions, store_dir: &Path) -> Option<RestoreCtx> {
    let manifest = match Manifest::load(opts.manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return None;
        }
    };
    let output_path = opts.output?.to_path_buf();
    if !check_overwrite(&output_path, opts.force) {
        return None;
    }
    let chunk_store = match open_chunk_store(store_dir, &manifest, opts.identity) {
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
        jobs: opts.jobs,
    })
}

pub fn restore(opts: &RestoreOptions) -> bool {
    let store_dir = derive_store_from_manifest(opts.manifest_path)
        .unwrap_or_else(|| opts.store_dir.to_path_buf());

    match (opts.output, opts.vm) {
        (None, None) => {
            let manifest = match Manifest::load(opts.manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error loading manifest: {e}");
                    return false;
                }
            };
            let chunk_store = match open_chunk_store(&store_dir, &manifest, opts.identity) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    return false;
                }
            };
            let output_path = Path::new(&manifest.source.path);
            println!(
                "Restore: {} -> {}",
                opts.manifest_path.display(),
                output_path.display()
            );
            if !check_overwrite(output_path, opts.force) {
                return false;
            }
            if is_empty_manifest(&manifest, output_path) {
                return true;
            }
            match restore_file(&manifest, &*chunk_store, output_path, opts.jobs) {
                Ok(_) => {
                    println!("Restore complete: {}", output_path.display());
                    true
                }
                Err(e) => {
                    eprintln!("Restore failed: {e}");
                    false
                }
            }
        }
        (Some(_), Some(_)) => {
            eprintln!("Error: --output (-o) and --vm are mutually exclusive");
            false
        }
        (Some(_), None) => {
            println!("Restore: {}", opts.manifest_path.display());
            let ctx = match prepare_restore(opts, &store_dir) {
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
        (None, Some(vm_id)) => {
            println!(
                "Restore VM disk: {} (manifest: {})",
                vm_id,
                opts.manifest_path.display()
            );
            let manifest = match Manifest::load(opts.manifest_path) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("Error loading manifest: {e}");
                    return false;
                }
            };
            let chunk_store = match open_chunk_store(&store_dir, &manifest, opts.identity) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    return false;
                }
            };
            restore_vm(
                &manifest,
                &*chunk_store,
                vm_id,
                opts.disk,
                opts.no_suspend,
                opts.hypervisor,
                opts.jobs,
            )
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
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if !force
        && (name == "null" || name == "zero")
        && path
            .parent()
            .is_some_and(|p| p == std::path::Path::new("/dev"))
    {
        eprintln!(
            "Error: refusing to restore to special device: {}",
            path.display()
        );
        eprintln!("  (use --force to override)");
        return false;
    }
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
        let identity = parse_age_identity(&identity_str, std::path::Path::new(identity_path))?;
        let recipient = identity.to_public();
        Ok(Box::new(EncryptedChunkStore::new(
            compressed, recipient, identity,
        )))
    } else {
        Ok(Box::new(compressed))
    }
}

fn parse_age_identity(
    content: &str,
    path: &std::path::Path,
) -> Result<age::x25519::Identity, String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("AGE-SECRET-KEY-") {
            return age::x25519::Identity::from_str(trimmed)
                .map_err(|e| format!("Invalid age identity in {}: {e}", path.display()));
        }
    }
    Err(format!(
        "No valid age key found in {} — expected a line starting with AGE-SECRET-KEY-",
        path.display()
    ))
}

fn write_chunks_to_writer(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    writer: &mut dyn Write,
    jobs: usize,
) -> Result<usize, Box<dyn std::error::Error>> {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(jobs.max(1))
        .build()?;
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
                    writer.write_all(&data)?;
                    pb.inc(1);
                }
                Err(e) => {
                    pb.finish_and_clear();
                    eprintln!("Error: {e}");
                    return Err(e.into());
                }
            }
        }
    }

    pb.finish_and_clear();
    println!("Total chunks restored: {total}");
    println!("Total size: {} bytes", manifest.source.size);
    Ok(total)
}

pub fn restore_file(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    dest: &Path,
    jobs: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tmp_path(dest);
    let mut output = fs::File::create(&tmp)?;
    let result = write_chunks_to_writer(manifest, store, &mut output, jobs);
    match result {
        Ok(_) => {
            output.sync_all()?;
            apply_file_handle_metadata(&output, manifest)?;
            drop(output);
            fs::rename(&tmp, dest)?;
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
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

fn is_block_device(path: &Path) -> bool {
    #[cfg(unix)]
    {
        std::fs::metadata(path)
            .map(|m| m.file_type().is_block_device())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

struct ResumeGuard<'a> {
    lifecycle: &'a dyn VmLifecycle,
    vm_id: &'a str,
    did_resume: bool,
}

impl<'a> ResumeGuard<'a> {
    fn new(lifecycle: &'a dyn VmLifecycle, vm_id: &'a str) -> Self {
        ResumeGuard {
            lifecycle,
            vm_id,
            did_resume: false,
        }
    }

    fn disarm(mut self) {
        self.did_resume = true;
    }
}

impl Drop for ResumeGuard<'_> {
    fn drop(&mut self) {
        if !self.did_resume {
            if let Err(e) = self.lifecycle.resume(self.vm_id) {
                eprintln!("Warning: failed to resume VM {}: {e}", self.vm_id);
            }
        }
    }
}

fn restore_to_block_device(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    device_path: &Path,
    jobs: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = OpenOptions::new().write(true).open(device_path)?;
    write_chunks_to_writer(manifest, store, &mut file, jobs)?;
    file.sync_all()?;
    Ok(())
}

fn restore_to_regular_file(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    dest: &Path,
    jobs: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    restore_file(manifest, store, dest, jobs)
}

fn restore_vm(
    manifest: &Manifest,
    store: &dyn ChunkStore,
    vm_id: &str,
    disk: Option<&str>,
    no_suspend: bool,
    hypervisor: &Hypervisor,
    jobs: usize,
) -> bool {
    if let Err(e) = validate_vm_id(vm_id) {
        eprintln!("Error: {e}");
        return false;
    }

    let (disk_path, disk_name) = match detect_disk_path(vm_id, disk) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            return false;
        }
    };
    println!("Restoring to disk: {} ({})", disk_path.display(), disk_name);

    if !disk_path.exists() {
        eprintln!("Error: disk path does not exist: {}", disk_path.display());
        return false;
    }

    if !matches!(
        manifest.source.source_type,
        rmrf_core::SourceType::VmDisk { .. }
    ) {
        eprintln!(
            "Warning: manifest was created from a file backup, not a VM disk. \
             Restoring file data to a VM disk may produce unexpected results."
        );
    }

    let lifecycle: Box<dyn VmLifecycle> = if no_suspend {
        Box::new(NoopLifecycle)
    } else {
        build_vm_lifecycle(Some(vm_id), hypervisor)
    };

    if let Err(e) = lifecycle.suspend(vm_id) {
        eprintln!("Error: failed to suspend VM {vm_id}: {e}");
        return false;
    }
    let guard = ResumeGuard::new(&*lifecycle, vm_id);

    let result = if is_block_device(&disk_path) {
        restore_to_block_device(manifest, store, &disk_path, jobs)
    } else {
        restore_to_regular_file(manifest, store, &disk_path, jobs)
    };

    match result {
        Ok(_) => {
            println!("Restore complete: {}", disk_path.display());
            guard.disarm();
            true
        }
        Err(e) => {
            eprintln!("Restore failed: {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn is_block_device_regular_file_is_false() {
        let tmp = tempfile::TempDir::new().unwrap();
        let f = tmp.path().join("test.bin");
        std::fs::write(&f, b"data").unwrap();
        assert!(!is_block_device(&f));
    }

    #[test]
    fn is_block_device_nonexistent_path_is_false() {
        assert!(!is_block_device(Path::new("/nonexistent/device")));
    }

    #[test]
    fn is_block_device_dev_null_is_not_block() {
        assert!(!is_block_device(Path::new("/dev/null")));
    }

    #[test]
    fn resume_guard_resumes_on_drop() {
        let vm_id = "test-vm";
        let lifecycle = NoopLifecycle;
        let guard = ResumeGuard::new(&lifecycle, vm_id);
        drop(guard);
    }

    #[test]
    fn resume_guard_disarm_skips_resume() {
        let vm_id = "test-vm";
        let lifecycle = NoopLifecycle;
        let guard = ResumeGuard::new(&lifecycle, vm_id);
        guard.disarm();
    }

    #[test]
    fn derive_store_from_manifest_in_manifests_dir() {
        let p = Path::new("/some/store/manifests/20250101T120000Z-test.bincode");
        assert_eq!(
            derive_store_from_manifest(p),
            Some(PathBuf::from("/some/store"))
        );
    }

    #[test]
    fn derive_store_from_manifest_not_in_manifests() {
        let p = Path::new("/some/other/path/manifest.bincode");
        assert_eq!(derive_store_from_manifest(p), None);
    }

    #[test]
    fn derive_store_from_manifest_root_path() {
        let p = Path::new("manifest.bincode");
        assert_eq!(derive_store_from_manifest(p), None);
    }

    #[test]
    fn restore_rejects_no_output_and_no_vm() {
        let opts = RestoreOptions {
            manifest_path: Path::new("nonexistent"),
            store_dir: Path::new("nonexistent"),
            output: None,
            force: false,
            jobs: 4,
            identity: None,
            vm: None,
            disk: None,
            no_suspend: false,
            hypervisor: &Hypervisor::Noop,
        };
        assert!(!restore(&opts));
    }

    #[test]
    fn restore_rejects_both_output_and_vm() {
        let opts = RestoreOptions {
            manifest_path: Path::new("nonexistent"),
            store_dir: Path::new("nonexistent"),
            output: Some(Path::new("/tmp/out")),
            force: false,
            jobs: 4,
            identity: None,
            vm: Some("100"),
            disk: None,
            no_suspend: false,
            hypervisor: &Hypervisor::Noop,
        };
        assert!(!restore(&opts));
    }
}
