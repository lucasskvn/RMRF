use rmrf_core::ChunkStore;
use rmrf_store::{hash_data, LocalChunkStore};

fn make_hash(val: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    h[0] = val;
    h
}

#[test]
fn store_put_and_get() {
    let tmp = std::env::temp_dir().join("rmrf_test_store_putget");
    let mut store = LocalChunkStore::new(tmp.clone()).unwrap();
    let data = b"hello chunk";
    let hash = hash_data(data);
    store.put(&hash, data).unwrap();
    let retrieved = store.get(&hash).unwrap().unwrap();
    assert_eq!(retrieved, data);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn store_get_missing() {
    let tmp = std::env::temp_dir().join("rmrf_test_store_missing");
    let store = LocalChunkStore::new(tmp.clone()).unwrap();
    let hash = make_hash(0xFF);
    assert!(store.get(&hash).unwrap().is_none());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn store_exists() {
    let tmp = std::env::temp_dir().join("rmrf_test_store_exists");
    let mut store = LocalChunkStore::new(tmp.clone()).unwrap();
    let hash = make_hash(42);
    assert!(!store.exists(&hash).unwrap());
    store.put(&hash, b"data").unwrap();
    assert!(store.exists(&hash).unwrap());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn store_delete() {
    let tmp = std::env::temp_dir().join("rmrf_test_store_delete");
    let mut store = LocalChunkStore::new(tmp.clone()).unwrap();
    let hash = make_hash(7);
    store.put(&hash, b"to delete").unwrap();
    assert!(store.exists(&hash).unwrap());
    store.delete(&hash).unwrap();
    assert!(!store.exists(&hash).unwrap());
    let _ = std::fs::remove_dir_all(&tmp);
}
