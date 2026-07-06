use rmrf_core::{
    ChunkLocation, ChunkStore, DedupIndex, Hash, IndexEntry, Result as SilexResult, SilexError,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use zstd::Encoder;

pub struct LocalChunkStore {
    base_path: PathBuf,
}

impl LocalChunkStore {
    pub fn new(path: PathBuf) -> SilexResult<Self> {
        fs::create_dir_all(&path)?;
        Ok(LocalChunkStore { base_path: path })
    }

    fn hash_to_path(&self, hash: &Hash) -> PathBuf {
        let hex = hex::encode(hash);
        let prefix = &hex[..2];
        let filename = &hex[2..];
        self.base_path.join(prefix).join(filename)
    }
}

impl ChunkStore for LocalChunkStore {
    fn put(&mut self, hash: &Hash, data: &[u8]) -> SilexResult<ChunkLocation> {
        let path = self.hash_to_path(hash);
        let parent = path
            .parent()
            .ok_or_else(|| SilexError::Msg("invalid chunk path: no parent directory".into()))?;
        fs::create_dir_all(parent)?;
        fs::write(&path, data)?;

        let prefix = parent
            .file_name()
            .ok_or_else(|| SilexError::Msg("invalid chunk path: no prefix directory".into()))?
            .to_string_lossy()
            .to_string();
        let filename = path
            .file_name()
            .ok_or_else(|| SilexError::Msg("invalid chunk path: no filename".into()))?
            .to_string_lossy()
            .to_string();

        Ok(ChunkLocation { prefix, filename })
    }

    fn get(&self, hash: &Hash) -> SilexResult<Option<Vec<u8>>> {
        let path = self.hash_to_path(hash);

        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn exists(&self, hash: &Hash) -> SilexResult<bool> {
        Ok(self.hash_to_path(hash).exists())
    }

    fn delete(&mut self, hash: &Hash) -> SilexResult<()> {
        fs::remove_file(self.hash_to_path(hash))?;
        Ok(())
    }
}

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
    fn lookup(&self, hash: &Hash) -> SilexResult<Option<IndexEntry>> {
        Ok(self.entries.get(&Self::hash_key(hash)).cloned())
    }

    fn insert(
        &mut self,
        hash: &Hash,
        location: ChunkLocation,
        uncompressed_size: u32,
        compressed_size: u32,
    ) -> SilexResult<()> {
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

    fn increment_refcount(&mut self, hash: &Hash) -> SilexResult<()> {
        if let Some(entry) = self.entries.get_mut(&Self::hash_key(hash)) {
            entry.refcount += 1;
        }
        Ok(())
    }

    fn decrement_refcount(&mut self, hash: &Hash) -> SilexResult<u64> {
        if let Some(entry) = self.entries.get_mut(&Self::hash_key(hash)) {
            entry.refcount = entry.refcount.saturating_sub(1);
            Ok(entry.refcount)
        } else {
            Ok(0)
        }
    }
}

pub struct CompressedChunkStore<S: ChunkStore> {
    inner: S,
    compression_level: i32,
}

impl<S: ChunkStore> CompressedChunkStore<S> {
    pub fn new(inner: S) -> Self {
        CompressedChunkStore {
            inner,
            compression_level: 10,
        }
    }

    fn compress(&self, data: &[u8]) -> SilexResult<Vec<u8>> {
        let mut encoder = Encoder::new(Vec::new(), self.compression_level)
            .map_err(|e| SilexError::Msg(format!("Compression error: {e}")))?;
        encoder
            .write_all(data)
            .map_err(|e| SilexError::Msg(format!("Write error: {e}")))?;
        encoder
            .finish()
            .map_err(|e| SilexError::Msg(format!("Finish error: {e}")))
    }

    fn decompress(&self, data: &[u8]) -> SilexResult<Vec<u8>> {
        let mut decoder = zstd::Decoder::new(data)
            .map_err(|e| SilexError::Msg(format!("Decompression error: {e}")))?;
        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| SilexError::Msg(format!("Read error: {e}")))?;
        Ok(result)
    }
}

impl<S: ChunkStore> ChunkStore for CompressedChunkStore<S> {
    fn put(&mut self, hash: &Hash, data: &[u8]) -> SilexResult<ChunkLocation> {
        let compressed = self.compress(data)?;
        self.inner.put(hash, &compressed)
    }

    fn get(&self, hash: &Hash) -> SilexResult<Option<Vec<u8>>> {
        match self.inner.get(hash)? {
            Some(compressed_data) => Ok(Some(self.decompress(&compressed_data)?)),
            None => Ok(None),
        }
    }

    fn exists(&self, hash: &Hash) -> SilexResult<bool> {
        self.inner.exists(hash)
    }

    fn delete(&mut self, hash: &Hash) -> SilexResult<()> {
        self.inner.delete(hash)
    }
}

pub fn hash_data(data: &[u8]) -> Hash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];

    hash.copy_from_slice(&result);
    hash
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
    fn store_put_and_get() {
        let tmp = std::env::temp_dir().join("rmrf_test_put_get");
        let mut store = LocalChunkStore::new(tmp.clone()).unwrap();
        let hash = hash_data(b"hello");
        store.put(&hash, b"hello").unwrap();
        assert!(store.exists(&hash).unwrap());
        let data = store.get(&hash).unwrap();
        assert_eq!(data, Some(b"hello".to_vec()));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn store_get_missing() {
        let tmp = std::env::temp_dir().join("rmrf_test_missing");
        let store = LocalChunkStore::new(tmp.clone()).unwrap();
        let result = store.get(&make_hash(99)).unwrap();
        assert_eq!(result, None);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn store_exists() {
        let tmp = std::env::temp_dir().join("rmrf_test_exists");
        let mut store = LocalChunkStore::new(tmp.clone()).unwrap();
        let hash = make_hash(1);
        assert!(!store.exists(&hash).unwrap());
        store.put(&hash, b"test").unwrap();
        assert!(store.exists(&hash).unwrap());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn store_delete() {
        let tmp = std::env::temp_dir().join("rmrf_test_delete");
        let mut store = LocalChunkStore::new(tmp.clone()).unwrap();
        let hash = make_hash(2);
        store.put(&hash, b"test").unwrap();
        assert!(store.exists(&hash).unwrap());
        store.delete(&hash).unwrap();
        assert!(!store.exists(&hash).unwrap());
        let _ = std::fs::remove_dir_all(&tmp);
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

    #[test]
    fn hash_data_is_deterministic() {
        let a = hash_data(b"test");
        let b = hash_data(b"test");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_data_different_for_different_data() {
        let a = hash_data(b"hello");
        let b = hash_data(b"world");
        assert_ne!(a, b);
    }

    #[test]
    fn compression_roundtrip() {
        let tmp = std::env::temp_dir().join("rmrf_test_compress_roundtrip");
        let local = LocalChunkStore::new(tmp.clone()).unwrap();
        let mut store = CompressedChunkStore::new(local);
        let data = b"Hello compression! ".repeat(100);
        let hash = hash_data(&data);
        store.put(&hash, &data).unwrap();
        let restored = store.get(&hash).unwrap().unwrap();
        assert_eq!(restored, data);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compression_reduces_size() {
        let tmp = std::env::temp_dir().join("rmrf_test_compress_ratio");
        let local = LocalChunkStore::new(tmp.clone()).unwrap();
        let mut store = CompressedChunkStore::new(local);
        let data = b"A".repeat(8192);
        let hash = hash_data(&data);
        store.put(&hash, &data).unwrap();
        let stored = store.inner.get(&hash).unwrap().unwrap();
        assert!(
            stored.len() < data.len(),
            "compressed {} >= original {}",
            stored.len(),
            data.len()
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn compression_exists_delegates() {
        let tmp = std::env::temp_dir().join("rmrf_test_compress_exists");
        let local = LocalChunkStore::new(tmp.clone()).unwrap();
        let mut store = CompressedChunkStore::new(local);
        let hash = make_hash(42);
        assert!(!store.exists(&hash).unwrap());
        store.put(&hash, b"test").unwrap();
        assert!(store.exists(&hash).unwrap());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
