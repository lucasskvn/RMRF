use rmrf_core::{Hash, Result, RmrfError, SourceMetadata};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const MANIFEST_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub source: SourceMetadata,
    pub chunks: Vec<Hash>,
}

impl Manifest {
    pub fn save(&self, path: &Path) -> Result<()> {
        let data = bincode::serialize(self)
            .map_err(|e| RmrfError::Msg(format!("manifest serialize: {e}")))?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let data = fs::read(path)?;
        let manifest: Self = bincode::deserialize(&data)
            .map_err(|e| RmrfError::Msg(format!("manifest deserialize: {e}")))?;
        if manifest.version != MANIFEST_VERSION {
            return Err(RmrfError::Msg(format!(
                "unsupported manifest version: {} (expected {MANIFEST_VERSION})",
                manifest.version
            )));
        }
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmrf_core::{SourceId, SourceType};

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
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            source: sample_source_metadata(),
            chunks: vec![[1u8; 32], [2u8; 32]],
        };
        let tmp = std::env::temp_dir().join("test_manifest_roundtrip.bincode");
        manifest.save(&tmp).unwrap();
        let loaded = Manifest::load(&tmp).unwrap();
        assert_eq!(loaded.version, manifest.version);
        assert_eq!(loaded.source.size, manifest.source.size);
        assert_eq!(loaded.chunks.len(), 2);
        assert_eq!(loaded.chunks[0], [1u8; 32]);
        assert_eq!(loaded.chunks[1], [2u8; 32]);
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn manifest_load_rejects_wrong_version() {
        let manifest = Manifest {
            version: 99,
            source: sample_source_metadata(),
            chunks: vec![],
        };
        let tmp = std::env::temp_dir().join("test_manifest_bad_version.bincode");
        manifest.save(&tmp).unwrap();
        let result = Manifest::load(&tmp);
        assert!(result.is_err());
        assert!(format!("{}", result.unwrap_err()).contains("unsupported manifest version"));
        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn manifest_load_missing_file() {
        let result = Manifest::load(Path::new("/tmp/does_not_exist_rrmf.bincode"));
        assert!(result.is_err());
    }

    #[test]
    fn manifest_empty_chunks_is_valid() {
        let manifest = Manifest {
            version: MANIFEST_VERSION,
            source: SourceMetadata {
                id: SourceId("empty".into()),
                path: "/dev/null".into(),
                size: 0,
                modified: 0,
                permissions: 0o000,
                source_type: SourceType::File,
            },
            chunks: vec![],
        };
        let tmp = std::env::temp_dir().join("test_manifest_empty.bincode");
        manifest.save(&tmp).unwrap();
        let loaded = Manifest::load(&tmp).unwrap();
        assert_eq!(loaded.chunks.len(), 0);
        assert_eq!(loaded.source.size, 0);
        let _ = fs::remove_file(&tmp);
    }
}
