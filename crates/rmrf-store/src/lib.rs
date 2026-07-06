mod compressed;
mod hash;
mod local;
mod memory_index;
mod persistent_index;

pub use compressed::CompressedChunkStore;
pub use hash::hash_data;
pub use local::LocalChunkStore;
pub use memory_index::MemoryDedupIndex;
pub use persistent_index::PersistentDedupIndex;