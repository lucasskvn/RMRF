use rmrf_core::{Source, SourceId, SourceMetadata, SourceType};
use std::fs::File;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;

pub struct FileSource {
    pub(crate) file: File,
    pub(crate) metadata: SourceMetadata,
}

impl FileSource {
    pub fn new(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let file_metadata = std::fs::metadata(path)?;
        let size = if file_metadata.file_type().is_block_device() {
            block_device_size(path)?
        } else {
            file_metadata.len()
        };
        let metadata = SourceMetadata {
            id: SourceId(path.to_string_lossy().to_string()),
            path: path.to_string_lossy().to_string(),
            size,
            modified: file_metadata.mtime(),
            permissions: file_metadata.mode(),
            source_type: SourceType::File,
        };
        Ok(FileSource { file, metadata })
    }
}

fn block_device_size(path: &Path) -> io::Result<u64> {
    let output = std::process::Command::new("blockdev")
        .args(["--getsize64", &path.to_string_lossy()])
        .output()
        .map_err(|e| io::Error::other(format!("blockdev: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!("blockdev --getsize64: {stderr}")));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .parse::<u64>()
        .map_err(|e| io::Error::other(format!("blockdev parse: {e}")))
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
