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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_hash(val: u8) -> Hash {
        let mut h = [0u8; 32];
        h[0] = val;
        h
    }

    #[test]
    fn index_insert_and_lookup() {
        let mut index = MemoryDedupIndex::new();
        let hash = make_hash(3);
        let loc = ChunkLocation {
            prefix: "ab".into(),
            filename: "cd.zst".into(),
        };
        index.insert(&hash, loc.clone(), 100, 50).unwrap();
        let entry = index.lookup(&hash).unwrap().unwrap();
        assert_eq!(entry.location.prefix, "ab");
        assert_eq!(entry.refcount, 1);
        assert_eq!(entry.uncompressed_size, 100);
    }

    #[test]
    fn index_increment_refcount() {
        let mut index = MemoryDedupIndex::new();
        let hash = make_hash(4);
        let loc = ChunkLocation {
            prefix: "x".into(),
            filename: "y".into(),
        };
        index.insert(&hash, loc, 1, 1).unwrap();
        index.increment_refcount(&hash).unwrap();
        let entry = index.lookup(&hash).unwrap().unwrap();
        assert_eq!(entry.refcount, 2);
    }

    #[test]
    fn index_decrement_refcount() {
        let mut index = MemoryDedupIndex::new();
        let hash = make_hash(5);
        let loc = ChunkLocation {
            prefix: "a".into(),
            filename: "b".into(),
        };
        index.insert(&hash, loc, 1, 1).unwrap();
        let count = index.decrement_refcount(&hash).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn index_decrement_missing_returns_zero() {
        let mut index = MemoryDedupIndex::new();
        let count = index.decrement_refcount(&make_hash(99)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn index_lookup_missing() {
        let index = MemoryDedupIndex::new();
        let result = index.lookup(&make_hash(99)).unwrap();
        assert_eq!(result, None);
    }
}