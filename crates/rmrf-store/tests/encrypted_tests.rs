use rmrf_core::{ChunkStore, Hash};
use rmrf_store::{EncryptedChunkStore, LocalChunkStore};
use tempfile::TempDir;

fn make_hash(val: u8) -> Hash {
    let mut h = [0u8; 32];
    h[0] = val;
    h
}

fn make_store() -> (TempDir, LocalChunkStore) {
    let dir = TempDir::new().unwrap();
    let store = LocalChunkStore::new(dir.path().join("chunks")).unwrap();
    (dir, store)
}

#[test]
fn encrypted_roundtrip() {
    let key = age::x25519::Identity::generate();
    let pubkey = key.to_public();
    let (_dir, inner) = make_store();
    let mut enc = EncryptedChunkStore::new(inner, pubkey, key);

    let hash = make_hash(42);
    let data = b"secret data for encryption test";
    enc.put(&hash, data).unwrap();

    let got = enc.get(&hash).unwrap().unwrap();
    assert_eq!(got, data);
}

#[test]
fn encrypted_get_missing() {
    let key = age::x25519::Identity::generate();
    let pubkey = key.to_public();
    let (_dir, inner) = make_store();
    let enc = EncryptedChunkStore::new(inner, pubkey, key);

    let hash = make_hash(99);
    assert!(enc.get(&hash).unwrap().is_none());
}

#[test]
fn encrypted_exists_delegates() {
    let key = age::x25519::Identity::generate();
    let pubkey = key.to_public();
    let (_dir, mut inner) = make_store();
    let mut enc = EncryptedChunkStore::new(inner, pubkey, key);

    let hash = make_hash(1);
    assert!(!enc.exists(&hash).unwrap());
    enc.put(&hash, b"test").unwrap();
    assert!(enc.exists(&hash).unwrap());
}
