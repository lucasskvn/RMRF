use indicatif::{ProgressBar, ProgressStyle};
use rmrf_core::ChunkStore;
use rmrf_manifest::Manifest;
use rmrf_store::{CompressedChunkStore, LocalChunkStore};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn restore(manifest_path: &Path, store_dir: &Path, output: Option<&Path>, force: bool) {
    println!("Restore: {}", manifest_path.display());

    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {}", e);
            return;
        }
    };

    let output_path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| resolve_output_path(manifest_path));

    if output_path.exists() && !force {
        eprintln!(
            "Error: output file already exists: {} (use --force to overwrite)",
            output_path.display()
        );
        return;
    }

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
        fs::write(&output_path, b"").ok();
        println!("Restored empty file: {}", output_path.display());
        return;
    }

    match restore_file(&manifest, &chunk_store, &output_path) {
        Ok(_) => println!("Restore complete: {}", output_path.display()),
        Err(e) => eprintln!("Restore failed: {}", e),
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
) -> Result<(), Box<dyn std::error::Error>> {
    let mut output = fs::File::create(dest)?;
    let total = manifest.chunks.len();

    let pb = create_progress_bar(total as u64);

    for hash in manifest.chunks.iter() {
        match store.get(hash)? {
            Some(data) => {
                output.write_all(&data)?;
                pb.inc(1);
            }
            None => {
                pb.finish_and_clear();
                eprintln!("Error: chunk {:?} not found in store", &hash[..4]);
                return Err("chunk not found".into());
            }
        }
    }
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
