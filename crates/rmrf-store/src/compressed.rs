use rmrf_core::{ChunkLocation, ChunkStore, Hash, Result as RmrfResult, RmrfError};
use std::io::{Read, Write};
use zstd::Encoder;

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

    fn compress(&self, data: &[u8]) -> RmrfResult<Vec<u8>> {
        let mut encoder = Encoder::new(Vec::new(), self.compression_level)
            .map_err(|e| RmrfError::Msg(format!("Compression error: {e}")))?;
        encoder
            .write_all(data)
            .map_err(|e| RmrfError::Msg(format!("Write error: {e}")))?;
        encoder
            .finish()
            .map_err(|e| RmrfError::Msg(format!("Finish error: {e}")))
    }

    fn decompress(&self, data: &[u8]) -> RmrfResult<Vec<u8>> {
        let mut decoder = zstd::Decoder::new(data)
            .map_err(|e| RmrfError::Msg(format!("Decompression error: {e}")))?;
        let mut result = Vec::new();
        decoder
            .read_to_end(&mut result)
            .map_err(|e| RmrfError::Msg(format!("Read error: {e}")))?;
        Ok(result)
    }
}

impl<S: ChunkStore> ChunkStore for CompressedChunkStore<S> {
    fn put(&mut self, hash: &Hash, data: &[u8]) -> RmrfResult<ChunkLocation> {
        let compressed = self.compress(data)?;
        self.inner.put(hash, &compressed)
    }

    fn get(&self, hash: &Hash) -> RmrfResult<Option<Vec<u8>>> {
        match self.inner.get(hash)? {
            Some(compressed_data) => Ok(Some(self.decompress(&compressed_data)?)),
            None => Ok(None),
        }
    }

    fn exists(&self, hash: &Hash) -> RmrfResult<bool> {
        self.inner.exists(hash)
    }

    fn delete(&mut self, hash: &Hash) -> RmrfResult<()> {
        self.inner.delete(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_data;
    use crate::local::LocalChunkStore;

    fn make_hash(val: u8) -> Hash {
        let mut h = [0u8; 32];
        h[0] = val;
        h
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