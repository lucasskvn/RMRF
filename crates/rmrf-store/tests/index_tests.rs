use rmrf_core::{ChunkLocation, DedupIndex};
use rmrf_store::{MemoryDedupIndex, PersistentDedupIndex};

fn make_hash(val: u8) -> [u8; 32] {
    let mut h = [0u8; 32];
    h[0] = val;
    h
}

fn make_location() -> ChunkLocation {
    ChunkLocation {
        prefix: "ab".into(),
        filename: "cd.zst".into(),
    }
}

#[test]
fn memory_index_insert_and_lookup() {
    let mut idx = MemoryDedupIndex::new();
    let hash = make_hash(1);
    let loc = make_location();
    idx.insert(&hash, loc, 100, 50).unwrap();
    let entry = idx.lookup(&hash).unwrap().unwrap();
    assert_eq!(entry.location.prefix, "ab");
    assert_eq!(entry.refcount, 1);
    assert_eq!(entry.uncompressed_size, 100);
}

#[test]
fn memory_index_increment_refcount() {
    let mut idx = MemoryDedupIndex::new();
    let hash = make_hash(2);
    let loc = make_location();
    idx.insert(&hash, loc, 100, 100).unwrap();
    idx.increment_refcount(&hash).unwrap();
    let entry = idx.lookup(&hash).unwrap().unwrap();
    assert_eq!(entry.refcount, 2);
}

#[test]
fn memory_index_decrement_refcount() {
    let mut idx = MemoryDedupIndex::new();
    let hash = make_hash(3);
    let loc = make_location();
    idx.insert(&hash, loc, 100, 100).unwrap();
    let remaining = idx.decrement_refcount(&hash).unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn memory_index_decrement_missing_returns_zero() {
    let mut idx = MemoryDedupIndex::new();
    let hash = make_hash(99);
    let remaining = idx.decrement_refcount(&hash).unwrap();
    assert_eq!(remaining, 0);
}

#[test]
fn memory_index_lookup_missing() {
    let idx = MemoryDedupIndex::new();
    let hash = make_hash(4);
    assert!(idx.lookup(&hash).unwrap().is_none());
}

#[test]
fn persistent_index_new_creates_empty() {
    let tmp = std::env::temp_dir().join("rmrf_test_persist_new");
    let _ = std::fs::remove_file(&tmp);
    let idx = PersistentDedupIndex::new(tmp.clone()).unwrap();
    let hash = make_hash(10);
    assert!(idx.lookup(&hash).unwrap().is_none());
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn persistent_index_save_load_roundtrip() {
    let tmp = std::env::temp_dir().join("rmrf_test_persist_roundtrip");
    let _ = std::fs::remove_file(&tmp);
    {
        let mut idx = PersistentDedupIndex::new(tmp.clone()).unwrap();
        let hash = make_hash(20);
        let loc = make_location();
        idx.insert(&hash, loc, 100, 50).unwrap();
        idx.save().unwrap();
    }
    let idx = PersistentDedupIndex::new(tmp.clone()).unwrap();
    let hash = make_hash(20);
    let entry = idx.lookup(&hash).unwrap().unwrap();
    assert_eq!(entry.refcount, 1);
    assert_eq!(entry.uncompressed_size, 100);
    assert_eq!(entry.compressed_size, 50);
    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn persistent_index_cross_backup_dedup() {
    let tmp = std::env::temp_dir().join("rmrf_test_persist_dedup");
    let _ = std::fs::remove_file(&tmp);
    {
        let mut idx = PersistentDedupIndex::new(tmp.clone()).unwrap();
        let hash = make_hash(30);
        let loc = make_location();
        idx.insert(&hash, loc, 64, 32).unwrap();
        assert_eq!(idx.lookup(&hash).unwrap().unwrap().refcount, 1);
        idx.save().unwrap();
    }
    {
        let mut idx = PersistentDedupIndex::new(tmp.clone()).unwrap();
        let hash = make_hash(30);
        assert!(idx.lookup(&hash).unwrap().is_some());
        idx.increment_refcount(&hash).unwrap();
        assert_eq!(idx.lookup(&hash).unwrap().unwrap().refcount, 2);
        idx.save().unwrap();
    }
    let idx = PersistentDedupIndex::new(tmp.clone()).unwrap();
    let hash = make_hash(30);
    assert_eq!(idx.lookup(&hash).unwrap().unwrap().refcount, 2);
    let _ = std::fs::remove_file(&tmp);
}
