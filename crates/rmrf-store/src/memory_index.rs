use rmrf_core::{ChunkLocation, DedupIndex, Hash, IndexEntry, Result as RmrfResult};
use std::collections::HashMap;

pub struct MemoryDedupIndex {
    entries: HashMap<String, IndexEntry>,
}

impl MemoryDedupIndex {
    pub fn new() -> Self {
        MemoryDedupIndex {
            entries: HashMap::new(),
        }
    }

    fn hash_key(hash: &Hash) -> String {
        hex::encode(hash)
    }
}

impl Default for MemoryDedupIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl DedupIndex for MemoryDedupIndex {
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
