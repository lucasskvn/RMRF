use rmrf_core::*;
use std::io;

#[test]
fn source_id_from_string() {
    let id = SourceId("vm-prod-01".into());
    assert_eq!(id.0, "vm-prod-01");
}

#[test]
fn source_type_file_variant() {
    match SourceType::File {
        SourceType::File => {}
        _ => panic!("expected File variant"),
    }
}

#[test]
fn source_metadata_construction() {
    let meta = SourceMetadata {
        id: SourceId("x".into()),
        path: "/tmp/test".into(),
        size: 1024,
        modified: 1_712_345_678,
        permissions: 0o644,
        source_type: SourceType::File,
    };
    assert_eq!(meta.size, 1024);
}

#[test]
fn chunk_location_construction() {
    let loc = ChunkLocation {
        prefix: "ab/cd".into(),
        filename: "abcdef1234.zst".into(),
    };
    assert_eq!(loc.prefix, "ab/cd");
}

#[test]
fn index_entry_fields() {
    let entry = IndexEntry {
        location: ChunkLocation {
            prefix: "ab".into(),
            filename: "hash.zst".into(),
        },
        refcount: 1,
        uncompressed_size: 16384,
        compressed_size: 7000,
    };
    assert_eq!(entry.refcount, 1);
}

#[test]
fn hash_type_is_32_bytes() {
    let hash: Hash = [0u8; 32];
    assert_eq!(hash.len(), 32);
}

#[test]
fn source_trait_methods() {
    fn _assert(_s: &dyn Source) {
        fn _metadata(s: &dyn Source) -> SourceMetadata {
            s.metadata()
        }
        fn _read(s: &mut dyn Source, b: &mut [u8]) -> io::Result<usize> {
            s.read(b)
        }
        fn _size_hint(s: &dyn Source) -> Option<u64> {
            s.size_hint()
        }
    }
}

#[test]
fn chunk_store_trait_methods() {
    fn _assert(_s: &mut dyn ChunkStore) {
        fn _put(s: &mut dyn ChunkStore, h: &Hash, d: &[u8]) -> Result<ChunkLocation> {
            s.put(h, d)
        }
        fn _get(s: &dyn ChunkStore, h: &Hash) -> Result<Option<Vec<u8>>> {
            s.get(h)
        }
        fn _exists(s: &dyn ChunkStore, h: &Hash) -> Result<bool> {
            s.exists(h)
        }
        fn _delete(s: &mut dyn ChunkStore, h: &Hash) -> Result<()> {
            s.delete(h)
        }
    }
}

#[test]
fn dedup_index_trait_methods() {
    fn _assert(_idx: &mut dyn DedupIndex) {
        fn _lookup(idx: &dyn DedupIndex, h: &Hash) -> Result<Option<IndexEntry>> {
            idx.lookup(h)
        }
        fn _insert(
            idx: &mut dyn DedupIndex,
            h: &Hash,
            loc: ChunkLocation,
            us: u32,
            cs: u32,
        ) -> Result<()> {
            idx.insert(h, loc, us, cs)
        }
        fn _incr(idx: &mut dyn DedupIndex, h: &Hash) -> Result<()> {
            idx.increment_refcount(h)
        }
        fn _decr(idx: &mut dyn DedupIndex, h: &Hash) -> Result<u64> {
            idx.decrement_refcount(h)
        }
    }
}

#[test]
fn error_type_display() {
    let err = RmrfError::Msg("test error".into());
    assert_eq!(format!("{}", err), "test error");
}

#[test]
fn result_type_alias() {
    fn _f() -> Result<()> {
        Ok(())
    }
}
