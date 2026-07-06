use rmrf_core::{ChunkLocation, DedupIndex, Hash, IndexEntry, Result as RmrfResult, RmrfError};
use std::collections::HashMap;
use std::fs;
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
        fs::write(&self.save_path, data)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hash(val: u8) -> Hash {
        let mut h = [0u8; 32];
        h[0] = val;
        h
    }

    #[test]
    fn persistent_index_new_creates_empty() {
        let tmp = std::env::temp_dir().join("rmrf_test_persist_new");
        let _ = std::fs::remove_dir_all(&tmp);
        let path = tmp.join("index.bincode");
        let idx = PersistentDedupIndex::new(path.clone()).unwrap();
        assert!(idx.lookup(&make_hash(1)).unwrap().is_none());
        idx.save().unwrap();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn persistent_index_save_load_roundtrip() {
        let tmp = std::env::temp_dir().join("rmrf_test_persist_roundtrip");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("index.bincode");
        let hash = make_hash(10);
        let loc = ChunkLocation {
            prefix: "ab".into(),
            filename: "cd.zst".into(),
        };

        {
            let mut idx = PersistentDedupIndex::new(path.clone()).unwrap();
            idx.insert(&hash, loc.clone(), 100, 50).unwrap();
            idx.save().unwrap();
        }

        {
            let idx = PersistentDedupIndex::new(path.clone()).unwrap();
            let entry = idx.lookup(&hash).unwrap().unwrap();
            assert_eq!(entry.refcount, 1);
            assert_eq!(entry.uncompressed_size, 100);
            assert_eq!(entry.compressed_size, 50);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn persistent_index_cross_backup_dedup() {
        let tmp = std::env::temp_dir().join("rmrf_test_persist_cross");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let path = tmp.join("index.bincode");
        let hash = make_hash(20);
        let loc = ChunkLocation {
            prefix: "xx".into(),
            filename: "yy.zst".into(),
        };

        {
            let mut idx = PersistentDedupIndex::new(path.clone()).unwrap();
            idx.insert(&hash, loc.clone(), 64, 32).unwrap();
            assert_eq!(idx.lookup(&hash).unwrap().unwrap().refcount, 1);
            idx.save().unwrap();
        }

        {
            let mut idx = PersistentDedupIndex::new(path.clone()).unwrap();
            assert!(idx.lookup(&hash).unwrap().is_some());
            idx.increment_refcount(&hash).unwrap();
            assert_eq!(idx.lookup(&hash).unwrap().unwrap().refcount, 2);
            idx.save().unwrap();
        }

        {
            let idx = PersistentDedupIndex::new(path.clone()).unwrap();
            assert_eq!(idx.lookup(&hash).unwrap().unwrap().refcount, 2);
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}