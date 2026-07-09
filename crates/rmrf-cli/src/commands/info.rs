use rmrf_manifest::Manifest;
use std::path::Path;

use super::helpers::format_size;

pub fn info(manifest_path: &Path, store_dir: &Path) -> bool {
    if !store_dir.exists() {
        eprintln!("Error: store directory not found: {}", store_dir.display());
        return false;
    }
    let manifest = match Manifest::load(manifest_path) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error loading manifest: {e}");
            return false;
        }
    };
    println!("Version:      v{}", manifest.version);
    println!("Source path:  {}", manifest.source.path);
    let size = format_size(manifest.source.size);
    println!("Source size:  {} ({})", manifest.source.size, size);
    println!("Source type:  {:?}", manifest.source.source_type);
    println!("Modified:     {}", manifest.source.modified);
    println!("Permissions:  {:o}", manifest.source.permissions);
    println!("Chunks:       {}", manifest.chunks.len());
    true
}
