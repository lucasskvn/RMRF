/*
** EPITECH PROJECT, 2026
** RMRF
** File description:
** lib.rs
*/

use rmrf_core::{Source, SourceId, SourceMetadata, SourceType};
use std::fs::File;
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub struct FileSource {
    file: File,
    metadata: SourceMetadata,
}

impl FileSource {
    pub fn new(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let file_metadata = std::fs::metadata(path)?;
        let size = file_metadata.len();
        let modified = file_metadata.mtime();
        let permissions = file_metadata.mode();
        let metadata = SourceMetadata {
            id: SourceId(path.to_string_lossy().to_string()),
            path: path.to_string_lossy().to_string(),
            size,
            modified,
            permissions,
            source_type: SourceType::File,
        };

        Ok(FileSource { file, metadata })
    }
}

impl Source for FileSource {
    fn metadata(&self) -> SourceMetadata {
        self.metadata.clone()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }

    fn size_hint(&self) -> Option<u64> {
        Some(self.metadata.size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn file_source_opens_existing_file() {
        let tmp = std::env::temp_dir().join("rmrf_test_opens.txt");
        std::fs::write(&tmp, "hello").unwrap();
        let source = FileSource::new(&tmp).unwrap();
        assert_eq!(source.metadata().size, 5);
        assert_eq!(source.metadata().source_type, SourceType::File);
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn file_source_reads_content() {
        let tmp = std::env::temp_dir().join("rmrf_test_read.txt");
        std::fs::write(&tmp, "hello world").unwrap();
        let mut source = FileSource::new(&tmp).unwrap();
        let mut buf = [0u8; 32];
        let n = source.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"hello world");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn file_source_size_hint() {
        let tmp = std::env::temp_dir().join("rmrf_test_size.txt");
        std::fs::write(&tmp, "1234567890").unwrap();
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
        let tmp = std::env::temp_dir().join("rmrf_test_meta.txt");
        {
            let mut f = File::create(&tmp).unwrap();
            f.write_all(b"test").unwrap();
        }
        let source = FileSource::new(&tmp).unwrap();
        let meta = source.metadata();
        assert!(meta.modified > 0, "modified should be a real timestamp");
        assert!(meta.permissions != 0, "permissions should be real");
        let _ = std::fs::remove_file(&tmp);
    }
}
