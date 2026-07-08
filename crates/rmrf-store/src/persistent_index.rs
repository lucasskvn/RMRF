// PersistentDedupIndex: in-memory HashMap persisted via bincode.
//
// Behavior: the entire index is kept in memory (HashMap<String, IndexEntry>) and
// serialized to disk with bincode on every save(). Each save() rewrites the whole
// file atomically (write to .tmp -> fsync -> rename -> parent fsync). There is no
// incremental write: the cost is O(n) in the number of unique chunks, and new()
// also loads the full index into memory.
//
// This is an acceptable trade-off for V1 (personal use, reasonable volumes): the
// implementation is simple, atomic, and has no external dependency. It should be
// revisited if the project ever needs to handle large-scale VM backup volumes — a
// transactional KV store (e.g. redb, sled, or SQLite) with per-entry persistence
// and incremental writes would be a better fit at that scale.

use rmrf_core::{ChunkLocation, DedupIndex, Hash, IndexEntry, Result as RmrfResult, RmrfError};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

pub struct PersistentDedupIndex {
    entries: HashMap<String, IndexEntry>,
    save_path: PathBuf,
}

impl PersistentDedupIndex {
    pub fn new(save_path: PathBuf) -> RmrfResult<Self> {
        let entries = if save_path.exists() {
            let data = fs::read(&save_path)?;
            bincode::deserialize(&data)
                .map_err(|e| RmrfError::Msg(format!("Failed to load dedup index: {e}")))?
        } else {
            HashMap::new()
        };
        Ok(Self { entries, save_path })
    }

    pub fn save(&self) -> RmrfResult<()> {
        if let Some(parent) = self.save_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = bincode::serialize(&self.entries)
            .map_err(|e| RmrfError::Msg(format!("Failed to serialize dedup index: {e}")))?;
        let tmp_path = self.save_path.with_extension("tmp");
        {
            let mut file = fs::File::create(&tmp_path)?;
            file.write_all(&data)?;
            file.sync_all()?;
        }
        fs::rename(&tmp_path, &self.save_path)?;
        if let Some(parent) = self.save_path.parent() {
            if let Ok(dir) = fs::File::open(parent) {
                dir.sync_all().ok();
            }
        }
        Ok(())
    }

    fn hash_key(hash: &Hash) -> String {
        hex::encode(hash)
    }
}

impl DedupIndex for PersistentDedupIndex {
    fn lookup(&self, hash: &Hash) -> RmrfResult<Option<IndexEntry>> {
        Ok(self.entries.get(&Self::hash_key(hash)).cloned())
    }

    fn insert(
        &mut self,
        hash: &Hash,
        location: ChunkLocation,
        uncompressed_size: u32,
        compressed_size: u32,
    ) -> RmrfResult<()> {
        self.entries.insert(
            Self::hash_key(hash),
            IndexEntry {
                location,
                refcount: 1,
                uncompressed_size,
                compressed_size,
            },
        );
        Ok(())
    }

    fn increment_refcount(&mut self, hash: &Hash) -> RmrfResult<()> {
        if let Some(entry) = self.entries.get_mut(&Self::hash_key(hash)) {
            entry.refcount += 1;
        }
        Ok(())
    }

    fn decrement_refcount(&mut self, hash: &Hash) -> RmrfResult<u64> {
        if let Some(entry) = self.entries.get_mut(&Self::hash_key(hash)) {
            entry.refcount = entry.refcount.saturating_sub(1);
            Ok(entry.refcount)
        } else {
            Ok(0)
        }
    }
}
