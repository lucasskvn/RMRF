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