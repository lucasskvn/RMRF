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

    pub fn with_level(inner: S, level: i32) -> Self {
        CompressedChunkStore {
            inner,
            compression_level: level,
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

    fn get_raw(&self, hash: &Hash) -> RmrfResult<Option<Vec<u8>>> {
        self.inner.get(hash)
    }
}
