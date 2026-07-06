use blake3::Hasher;
use rmrf_core::Hash;

pub fn hash_data(data: &[u8]) -> Hash {
    let mut hasher = Hasher::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(result.as_bytes());
    hash
}