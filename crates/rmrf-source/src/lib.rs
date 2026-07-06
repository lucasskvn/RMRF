/*
** EPITECH PROJECT, 2026
** RMRF
** File description:
** lib.rs
*/

use rmrf_core::{DiskFormat, Source, SourceId, SourceMetadata, SourceType};
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

// ── FileSource ───────────────────────────────────────────────────

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

// ── VmLifecycle ──────────────────────────────────────────────────

pub trait VmLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()>;
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()>;
}

impl VmLifecycle for Box<dyn VmLifecycle> {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        (**self).suspend(vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        (**self).resume(vm_id)
    }
}

/// NoopLifecycle: does nothing, for tests and simulation
pub struct NoopLifecycle;

impl VmLifecycle for NoopLifecycle {
    fn suspend(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }

    fn resume(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }
}

/// ProxmoxLifecycle: uses qm suspend/resume
pub struct ProxmoxLifecycle;

impl VmLifecycle for ProxmoxLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        let output = Command::new("qm")
            .args(["suspend", vm_id])
            .output()
            .map_err(|e| rmrf_core::RmrfError::Msg(format!("qm suspend: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(rmrf_core::RmrfError::Msg(format!("qm suspend: {stderr}")));
        }
        Ok(())
    }

    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        let output = Command::new("qm")
            .args(["resume", vm_id])
            .output()
            .map_err(|e| rmrf_core::RmrfError::Msg(format!("qm resume: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(rmrf_core::RmrfError::Msg(format!("qm resume: {stderr}")));
        }
        Ok(())
    }
}

/// LibvirtLifecycle: uses virsh suspend/resume
pub struct LibvirtLifecycle;

impl VmLifecycle for LibvirtLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        let output = Command::new("virsh")
            .args(["suspend", vm_id])
            .output()
            .map_err(|e| rmrf_core::RmrfError::Msg(format!("virsh suspend: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(rmrf_core::RmrfError::Msg(format!("virsh suspend: {stderr}")));
        }
        Ok(())
    }

    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        let output = Command::new("virsh")
            .args(["resume", vm_id])
            .output()
            .map_err(|e| rmrf_core::RmrfError::Msg(format!("virsh resume: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(rmrf_core::RmrfError::Msg(format!("virsh resume: {stderr}")));
        }
        Ok(())
    }
}

/// VmSource wraps a FileSource with VM lifecycle management.
/// On open: suspends VM, opens file. On drop: resumes VM.
/// Fail-fast: if the file path doesn't exist, suspend is never called.
pub struct VmSource<L: VmLifecycle> {
    source: FileSource,
    lifecycle: L,
    vm_id: String,
    suspended: bool,
}

impl<L: VmLifecycle> VmSource<L> {
    pub fn open(
        path: &Path,
        vm_id: &str,
        disk_name: &str,
        format: DiskFormat,
        lifecycle: L,
    ) -> rmrf_core::Result<Self> {
        if !path.exists() {
            return Err(rmrf_core::RmrfError::Msg(format!(
                "VM disk not found: {}",
                path.display()
            )));
        }

        lifecycle.suspend(vm_id)?;

        let file_source = FileSource::new(path).map_err(|e| {
            let _ = lifecycle.resume(vm_id);
            rmrf_core::RmrfError::Io(e)
        })?;

        let mut vm_source = Self {
            source: file_source,
            lifecycle,
            vm_id: vm_id.to_string(),
            suspended: true,
        };

        vm_source.source.metadata.source_type = SourceType::VmDisk {
            vm_id: vm_id.to_string(),
            disk_name: disk_name.to_string(),
            format,
        };

        Ok(vm_source)
    }
}

impl<L: VmLifecycle> io::Read for VmSource<L> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.source.read(buf)
    }
}

impl<L: VmLifecycle> Source for VmSource<L> {
    fn metadata(&self) -> SourceMetadata {
        self.source.metadata()
    }

    fn size_hint(&self) -> Option<u64> {
        self.source.size_hint()
    }
}

impl<L: VmLifecycle> Drop for VmSource<L> {
    fn drop(&mut self) {
        if self.suspended {
            let _ = self.lifecycle.resume(&self.vm_id);
            self.suspended = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::rc::Rc;

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

    // ── MockLifecycle for tests ──────────────────────────────────

    struct MockLifecycle {
        suspended: Rc<RefCell<bool>>,
        resume_calls: Rc<RefCell<u32>>,
        should_fail_suspend: bool,
    }

    impl MockLifecycle {
        fn new(
            suspended: Rc<RefCell<bool>>,
            resume_calls: Rc<RefCell<u32>>,
        ) -> Self {
            Self {
                suspended,
                resume_calls,
                should_fail_suspend: false,
            }
        }

        fn failing() -> Self {
            Self {
                suspended: Rc::new(RefCell::new(false)),
                resume_calls: Rc::new(RefCell::new(0)),
                should_fail_suspend: true,
            }
        }
    }

    impl VmLifecycle for MockLifecycle {
        fn suspend(&self, _vm_id: &str) -> rmrf_core::Result<()> {
            if self.should_fail_suspend {
                return Err(rmrf_core::RmrfError::Msg("mock suspend failed".into()));
            }
            *self.suspended.borrow_mut() = true;
            Ok(())
        }

        fn resume(&self, _vm_id: &str) -> rmrf_core::Result<()> {
            *self.suspended.borrow_mut() = false;
            *self.resume_calls.borrow_mut() += 1;
            Ok(())
        }
    }

    fn create_test_file(name: &str, content: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    // ── VmSource tests ───────────────────────────────────────────

    #[test]
    fn noop_lifecycle_does_nothing() {
        let lifecycle = NoopLifecycle;
        assert!(lifecycle.suspend("100").is_ok());
        assert!(lifecycle.resume("100").is_ok());
    }

    #[test]
    fn vm_source_opens_with_noop_lifecycle() {
        let path = create_test_file("rmrf_vm_open.bin", b"vm data");
        let vm = VmSource::open(&path, "100", "disk0", DiskFormat::Raw, NoopLifecycle).unwrap();
        let meta = vm.metadata();
        assert_eq!(meta.size, 7);
        match &meta.source_type {
            SourceType::VmDisk { vm_id, disk_name, format } => {
                assert_eq!(vm_id, "100");
                assert_eq!(disk_name, "disk0");
                assert_eq!(*format, DiskFormat::Raw);
            }
            _ => panic!("expected VmDisk"),
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_source_reads_content() {
        let path = create_test_file("rmrf_vm_read.bin", b"disk content here");
        let mut vm = VmSource::open(&path, "200", "vda", DiskFormat::Qcow2, NoopLifecycle).unwrap();
        let mut buf = [0u8; 32];
        let n = vm.read(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"disk content here");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_source_drop_calls_resume() {
        let path = create_test_file("rmrf_vm_drop.bin", b"test");
        let suspended = Rc::new(RefCell::new(false));
        let resume_calls = Rc::new(RefCell::new(0));
        let lifecycle = MockLifecycle::new(Rc::clone(&suspended), Rc::clone(&resume_calls));

        {
            let _vm = VmSource::open(&path, "300", "hda", DiskFormat::Raw, lifecycle).unwrap();
            assert!(*suspended.borrow(), "should be suspended after open");
            assert_eq!(*resume_calls.borrow(), 0);
        }
        assert!(!*suspended.borrow(), "should be resumed after drop");
        assert_eq!(*resume_calls.borrow(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_source_suspend_failure_propagates() {
        let path = create_test_file("rmrf_vm_suspend_fail.bin", b"test");
        let result = VmSource::open(&path, "400", "sda", DiskFormat::Raw, MockLifecycle::failing());
        assert!(result.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_source_open_failure_after_suspend_resumes() {
        use std::os::unix::fs::PermissionsExt;

        let suspended = Rc::new(RefCell::new(false));
        let resume_calls = Rc::new(RefCell::new(0));
        let lifecycle = MockLifecycle::new(Rc::clone(&suspended), Rc::clone(&resume_calls));

        let path = std::env::temp_dir().join("rmrf_vm_no_read.bin");
        std::fs::write(&path, b"test").unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o000);
        std::fs::set_permissions(&path, perms).unwrap();

        let result = VmSource::open(&path, "500", "vdb", DiskFormat::Qcow2, lifecycle);
        assert!(result.is_err());
        assert!(!*suspended.borrow(), "should be resumed after open failure");
        assert_eq!(*resume_calls.borrow(), 1, "resume should be called exactly once");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn vm_source_file_not_found_before_suspend() {
        let suspended = Rc::new(RefCell::new(false));
        let resume_calls = Rc::new(RefCell::new(0));
        let lifecycle = MockLifecycle::new(Rc::clone(&suspended), Rc::clone(&resume_calls));

        let missing = Path::new("/tmp/rmrf_vm_does_not_exist.bin");
        if missing.exists() {
            let _ = std::fs::remove_file(missing);
        }

        let result = VmSource::open(missing, "600", "vdc", DiskFormat::Raw, lifecycle);
        assert!(result.is_err());
        assert!(!*suspended.borrow(), "suspend should NOT have been called");
        assert_eq!(*resume_calls.borrow(), 0);
    }

    #[test]
    fn vm_source_qcow2_format_metadata() {
        let path = create_test_file("rmrf_vm_qcow2.bin", b"qcow2 test");
        let vm = VmSource::open(&path, "700", "nvme0", DiskFormat::Qcow2, NoopLifecycle).unwrap();
        match &vm.metadata().source_type {
            SourceType::VmDisk { format, .. } => {
                assert_eq!(*format, DiskFormat::Qcow2);
            }
            _ => panic!("expected VmDisk"),
        }
        let _ = std::fs::remove_file(&path);
    }
}
