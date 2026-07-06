use rmrf_core::{ChunkLocation, ChunkStore, Hash, Result as RmrfResult, RmrfError};
use std::fs;
use std::path::PathBuf;

#[cfg(test)]
use crate::hash_data;

pub struct LocalChunkStore {
    base_path: PathBuf,
}

impl LocalChunkStore {
    pub fn new(path: PathBuf) -> RmrfResult<Self> {
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
    fn put(&mut self, hash: &Hash, data: &[u8]) -> RmrfResult<ChunkLocation> {
        let path = self.hash_to_path(hash);
        let parent = path
            .parent()
            .ok_or_else(|| RmrfError::Msg("invalid chunk path: no parent directory".into()))?;
        fs::create_dir_all(parent)?;
        fs::write(&path, data)?;

        let prefix = parent
            .file_name()
            .ok_or_else(|| RmrfError::Msg("invalid chunk path: no prefix directory".into()))?
            .to_string_lossy()
            .to_string();
        let filename = path
            .file_name()
            .ok_or_else(|| RmrfError::Msg("invalid chunk path: no filename".into()))?
            .to_string_lossy()
            .to_string();

        Ok(ChunkLocation { prefix, filename })
    }

    fn get(&self, hash: &Hash) -> RmrfResult<Option<Vec<u8>>> {
        let path = self.hash_to_path(hash);

        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn exists(&self, hash: &Hash) -> RmrfResult<bool> {
        Ok(self.hash_to_path(hash).exists())
    }

    fn delete(&mut self, hash: &Hash) -> RmrfResult<()> {
        fs::remove_file(self.hash_to_path(hash))?;
        Ok(())
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
}