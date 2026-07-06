use rmrf_manifest::Manifest;
use std::path::Path;

use super::helpers::format_size;

pub fn info(manifest_path: &Path, _store_dir: &Path) {
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return;
        }
    };
    println!("Version:     v{}", manifest.version);
    println!("Source path: {}", manifest.source.path);
    println!(
        "Source size: {} ({})",
        manifest.source.size,
        format_size(manifest.source.size)
    );
    println!("Source type: {:?}", manifest.source.source_type);
    println!("Modified:    {}", manifest.source.modified);
    println!("Permissions: {:o}", manifest.source.permissions);
    println!("Chunks:      {}", manifest.chunks.len());
}