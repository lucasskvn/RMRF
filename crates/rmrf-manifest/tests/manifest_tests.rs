use rmrf_core::{SourceId, SourceMetadata, SourceType};
use rmrf_manifest::{Manifest, MANIFEST_VERSION};
use std::path::Path;

fn sample_source_metadata() -> SourceMetadata {
    SourceMetadata {
        id: SourceId("/tmp/test".into()),
        path: "/tmp/test".into(),
        size: 1024,
        modified: 1_712_345_678,
        permissions: 0o644,
        source_type: SourceType::File,
    }
}

#[test]
fn manifest_save_load_roundtrip() {
    let manifest = Manifest::new(sample_source_metadata(), vec![[1u8; 32], [2u8; 32]], None);
    let tmp = std::env::temp_dir().join("test_manifest_roundtrip.bincode");
    manifest.save(&tmp).unwrap();
    let loaded = Manifest::load(&tmp).unwrap();
    assert_eq!(loaded.version, manifest.version);
    assert_eq!(loaded.source.size, manifest.source.size);
    assert_eq!(loaded.chunks.len(), 2);
    assert_eq!(loaded.chunks[0], [1u8; 32]);
    assert_eq!(loaded.chunks[1], [2u8; 32]);
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn manifest_load_rejects_wrong_version() {
    let manifest = Manifest {
        version: 99,
        source: sample_source_metadata(),
        chunks: vec![],
        encryption: None,
    };
    let tmp = std::env::temp_dir().join("test_manifest_bad_version.bincode");
    manifest.save(&tmp).unwrap();
    let result = Manifest::load(&tmp);
    assert!(result.is_err());
    assert!(format!("{}", result.unwrap_err()).contains("unsupported manifest version"));
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn manifest_load_missing_file() {
    let result = Manifest::load(Path::new("/tmp/does_not_exist_rrmf.bincode"));
    assert!(result.is_err());
}

#[test]
fn manifest_empty_chunks_is_valid() {
    let manifest = Manifest::new(
        SourceMetadata {
            id: SourceId("empty".into()),
            path: "/dev/null".into(),
            size: 0,
            modified: 0,
            permissions: 0o000,
            source_type: SourceType::File,
        },
        vec![],
        None,
    );
    let tmp = std::env::temp_dir().join("test_manifest_empty.bincode");
    manifest.save(&tmp).unwrap();
    let loaded = Manifest::load(&tmp).unwrap();
    assert_eq!(loaded.chunks.len(), 0);
    assert_eq!(loaded.source.size, 0);
    let _ = std::fs::remove_file(&tmp);
}
