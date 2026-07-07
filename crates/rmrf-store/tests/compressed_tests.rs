use rmrf_core::ChunkStore;
use rmrf_store::{hash_data, CompressedChunkStore, LocalChunkStore};

fn make_hash(val: u8) -> [u8; 32] {
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
    let stored = store.get_raw(&hash).unwrap().unwrap();
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
