/*
** EPITECH PROJECT, 2026
** RMRF
** File description:
** lib.rs
*/

use rmrf_core::{Source, SourceId, SourceMetadata, SourceType};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub struct FileSource {
    file: File,
    metadata: SourceMetadata,
}

impl FileSource {

    pub fn new(path: &Path) -> io::Result<Self>
    {
        let file = File::open(path)?;
        let file_metadata = std::fs::metadata(path)?;
        let size = file_metadata.len();
        let metadata = SourceMetadata {
            id: SourceId(path.to_string_lossy().to_string()),
            path: path.to_string_lossy().to_string(),
            size,
            modified: 0,
            permissions: 0o644,
            source_type: SourceType::File,
        };

        Ok(FileSource {file, metadata })
    }
}

impl Source for FileSource {

    fn metadata(&self) -> SourceMetadata
    {
        self.metadata.clone()
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>
    {
        self.file.read(buf)
    }

    fn size_hint(&self) -> Option<u64>
    {
        Some(self.metadata.size)
    }
}
