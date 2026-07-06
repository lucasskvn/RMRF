use rmrf_source::FileSource;
use rmrf_core::Source;
use std::io::Read;
use std::io::Write;

fn create_test_file(name: &str, content: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content).unwrap();
    path
}

#[test]
fn file_source_opens_existing_file() {
    let path = create_test_file("rmrf_test_fsrc_open", b"hello");
    let source = FileSource::new(&path);
    assert!(source.is_ok());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_source_reads_content() {
    let path = create_test_file("rmrf_test_fsrc_read", b"hello world");
    let mut source = FileSource::new(&path).unwrap();
    let mut buf = [0u8; 64];
    let n = source.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"hello world");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_source_size_hint() {
    let path = create_test_file("rmrf_test_fsrc_size", b"1234567890");
    let source = FileSource::new(&path).unwrap();
    assert_eq!(source.size_hint(), Some(10));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn file_source_error_on_missing() {
    let result = FileSource::new(std::path::Path::new("/tmp/nonexistent_rmrf_test_999"));
    assert!(result.is_err());
}

#[test]
fn file_source_reads_real_metadata() {
    let path = create_test_file("rmrf_test_fsrc_meta", b"metadata test");
    let source = FileSource::new(&path).unwrap();
    let meta = source.metadata();
    assert_eq!(meta.size, 13);
    assert_eq!(meta.path, path.to_string_lossy());
    assert!(meta.modified > 0);
    let _ = std::fs::remove_file(&path);
}
