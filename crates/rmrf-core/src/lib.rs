use serde::{Deserialize, Serialize};
use std::io;

pub type Result<T> = std::result::Result<T, RmrfError>;

#[derive(Debug, thiserror::Error)]
pub enum RmrfError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Msg(String),
}

pub type Hash = [u8; 32];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SourceType {
    File,
    VmDisk {
        vm_id: String,
        disk_name: String,
        format: DiskFormat,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DiskFormat {
    Raw,
    Qcow2,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceMetadata {
    pub id: SourceId,
    pub path: String,
    pub size: u64,
    pub modified: i64,
    pub permissions: u32,
    pub source_type: SourceType,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkLocation {
    pub prefix: String,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexEntry {
    pub location: ChunkLocation,
    pub refcount: u64,
    pub uncompressed_size: u32,
    pub compressed_size: u32,
}

pub trait Source: io::Read {
    fn metadata(&self) -> SourceMetadata;
    fn size_hint(&self) -> Option<u64>;
}

pub trait ChunkStore: Send + Sync {
    fn put(&mut self, hash: &Hash, data: &[u8]) -> Result<ChunkLocation>;
    fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>>;
    fn exists(&self, hash: &Hash) -> Result<bool>;
    fn delete(&mut self, hash: &Hash) -> Result<()>;
    fn get_raw(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        self.get(hash)
    }
}

pub trait DedupIndex {
    fn lookup(&self, hash: &Hash) -> Result<Option<IndexEntry>>;
    fn insert(
        &mut self,
        hash: &Hash,
        location: ChunkLocation,
        uncompressed_size: u32,
        compressed_size: u32,
    ) -> Result<()>;
    fn increment_refcount(&mut self, hash: &Hash) -> Result<()>;
    fn decrement_refcount(&mut self, hash: &Hash) -> Result<u64>;
}
