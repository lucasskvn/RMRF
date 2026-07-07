use rmrf_core::{ChunkLocation, ChunkStore, Hash, Result as RmrfResult, RmrfError};

pub struct EncryptedChunkStore<S: ChunkStore> {
    inner: S,
    recipient: age::x25519::Recipient,
    identity: age::x25519::Identity,
}

impl<S: ChunkStore> EncryptedChunkStore<S> {
    pub fn new(
        inner: S,
        recipient: age::x25519::Recipient,
        identity: age::x25519::Identity,
    ) -> Self {
        EncryptedChunkStore {
            inner,
            recipient,
            identity,
        }
    }
}

impl<S: ChunkStore> ChunkStore for EncryptedChunkStore<S> {
    fn put(&mut self, hash: &Hash, data: &[u8]) -> RmrfResult<ChunkLocation> {
        let encrypted = age::encrypt(&self.recipient, data)
            .map_err(|e| RmrfError::Msg(format!("encryption failed: {e}")))?;
        self.inner.put(hash, &encrypted)
    }

    fn get(&self, hash: &Hash) -> RmrfResult<Option<Vec<u8>>> {
        match self.inner.get(hash)? {
            Some(encrypted) => {
                let decrypted = age::decrypt(&self.identity, &encrypted)
                    .map_err(|e| RmrfError::Msg(format!("decryption failed: {e}")))?;
                Ok(Some(decrypted))
            }
            None => Ok(None),
        }
    }

    fn get_raw(&self, hash: &Hash) -> RmrfResult<Option<Vec<u8>>> {
        self.inner.get_raw(hash)
    }

    fn exists(&self, hash: &Hash) -> RmrfResult<bool> {
        self.inner.exists(hash)
    }

    fn delete(&mut self, hash: &Hash) -> RmrfResult<()> {
        self.inner.delete(hash)
    }
}
