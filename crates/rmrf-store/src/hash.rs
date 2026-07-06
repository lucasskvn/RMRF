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

#[cfg(test)]
mod tests {
    use super::*;

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
}