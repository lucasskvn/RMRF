use rmrf_core::{ChunkLocation, ChunkStore, Hash, Result as RmrfResult, RmrfError};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

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
        let mut file = fs::File::create(&path)?;
        file.write_all(data)?;
        file.sync_all()?;

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
