use rmrf_core::{Source, SourceId, SourceMetadata, SourceType};
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub struct FileSource {
    pub(crate) file: File,
    pub(crate) metadata: SourceMetadata,
}

impl FileSource {
    pub fn new(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let file_metadata = std::fs::metadata(path)?;
        let metadata = SourceMetadata {
            id: SourceId(path.to_string_lossy().to_string()),
            path: path.to_string_lossy().to_string(),
            size: file_metadata.len(),
            modified: file_metadata.mtime(),
            permissions: file_metadata.mode(),
            source_type: SourceType::File,
        };
        Ok(FileSource { file, metadata })
    }
}

impl io::Read for FileSource {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }
}

impl Source for FileSource {
    fn metadata(&self) -> SourceMetadata {
        self.metadata.clone()
    }

    fn size_hint(&self) -> Option<u64> {
        Some(self.metadata.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn create_test_file(name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let mut f = File::create(&path).unwrap();
        f.write_all(content).unwrap();
        path
    }

    #[test]
    fn file_source_opens_existing_file() {
        let tmp = create_test_file("rmrf_test_opens.txt", b"hello");
        let source = FileSource::new(&tmp).unwrap();
        assert_eq!(source.metadata().size, 5);
        assert_eq!(source.metadata().source_type, SourceType::File);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn file_source_reads_content() {
        let tmp = create_test_file("rmrf_test_read.txt", b"hello world");
        let mut source = FileSource::new(&tmp).unwrap();
        let mut buf = [0u8; 32];
        use std::io::Read;
        let n = source.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello world");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn file_source_size_hint() {
        let tmp = create_test_file("rmrf_test_size.txt", b"1234567890");
        let source = FileSource::new(&tmp).unwrap();
        assert_eq!(source.size_hint(), Some(10));
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn file_source_error_on_missing() {
        let result = FileSource::new(Path::new("/tmp/does_not_exist_rmrf.txt"));
        assert!(result.is_err());
    }

    #[test]
    fn file_source_reads_real_metadata() {
        let tmp = create_test_file("rmrf_test_meta.txt", b"test");
        let source = FileSource::new(&tmp).unwrap();
        let meta = source.metadata();
        assert!(meta.modified > 0, "modified should be a real timestamp");
        assert_ne!(meta.permissions, 0, "permissions should be real");
        let _ = std::fs::remove_file(&tmp);
    }
}