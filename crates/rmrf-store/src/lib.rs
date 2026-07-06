use rmrf_core::{ChunkLocation, ChunkStore, DedupIndex, Hash, IndexEntry, Result as SilexResult};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

pub struct LocalChunkStore
{
    base_path: PathBuf,
}

impl LocalChunkStore {

    pub fn new(path:PathBuf) -> SilexResult<Self>
    {
        fs::create_dir_all(&path)?;
        Ok(LocalChunkStore {base_path: path })
    }

    fn hash_to_path(&self, hash: &Hash) -> PathBuf
    {
        let hex = hex::encode(hash);
        let prefix = &hex[..2];
        let filename = &hex[2..];
        self.base_path.join(prefix).join(filename)
    }
}

impl ChunkStore for LocalChunkStore {

    fn put(&mut self, hash: &Hash, data: &[u8]) -> SilexResult<ChunkLocation>
    {
        let path = self.hash_to_path(hash);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(&path, data)?;

        Ok(ChunkLocation {
            prefix: path.parent().unwrap().file_name().unwrap().to_string_lossy().to_string(),
            filename: path.file_name().unwrap().to_string_lossy().to_string(),
        })
    }

    fn get(&self, hash: &Hash) -> SilexResult<Option<Vec<u8>>>
    {
        let path = self.hash_to_path(hash);

        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn exists(&self, hash: &Hash) -> SilexResult<bool>
    {
        Ok(self.hash_to_path(hash).exists())
    }

    fn delete(&mut self, hash: &Hash) -> SilexResult<()>
    {
        fs::remove_file(self.hash_to_path(hash))?;
        Ok(())
    }
}

pub struct MemoryDedupIndex
{
    entries: HashMap<String, IndexEntry>,
}

impl MemoryDedupIndex {

    pub fn new() -> Self
    {
        MemoryDedupIndex { entries: HashMap::new(),}
    }

    fn hash_key(hash: &Hash) -> String
    {
        hex::encode(hash)
    }
}

impl DedupIndex for MemoryDedupIndex {

    fn lookup(&self,hash: &Hash) -> SilexResult<Option<IndexEntry>>
    {
        Ok(self.entries.get(&Self::hash_key(hash)).cloned())
    }

    fn insert(&mut self, hash: &Hash, location: ChunkLocation, uncompressed_size: u32,compressed_size: u32, ) -> SilexResult<()> 
    {
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

    fn increment_refcount(&mut self, hash: &Hash) -> SilexResult<()>
    {
        if let Some(entry) = self.entries.get_mut(&Self::hash_key(hash)) {
            entry.refcount += 1;
        }
        Ok(())
    }

    fn decrement_refcount(&mut self, hash: &Hash) -> SilexResult<u64>
    {
        if let Some(entry) = self.entries.get_mut(&Self::hash_key(hash)) {
            entry.refcount = entry.refcount.saturating_sub(1);
            Ok(entry.refcount)
        } else {
            Ok(0)
        }
    }
}

pub fn hash_data(data: &[u8]) -> Hash
{
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];

    hash.copy_from_slice(&result);
    hash
}
