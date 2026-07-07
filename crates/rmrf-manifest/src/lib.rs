use rmrf_core::{Hash, Result, RmrfError, SourceMetadata};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
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
        let mut file = fs::File::create(path)?;
        file.write_all(&data)?;
        file.sync_all()?;
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
