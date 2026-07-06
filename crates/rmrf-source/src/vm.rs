use crate::FileSource;
use rmrf_core::{DiskFormat, Source, SourceMetadata, SourceType};
use std::io;
use std::path::Path;
use std::process::Command;

// ── VmLifecycle trait ────────────────────────────────────────────

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

// ── NoopLifecycle ───────────────────────────────────────────────

pub struct NoopLifecycle;

impl VmLifecycle for NoopLifecycle {
    fn suspend(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }
    fn resume(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }
}

// ── ProxmoxLifecycle ────────────────────────────────────────────

pub struct ProxmoxLifecycle;

impl VmLifecycle for ProxmoxLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_qm("suspend", vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_qm("resume", vm_id)
    }
}

fn run_qm(action: &str, vm_id: &str) -> rmrf_core::Result<()> {
    let output = Command::new("qm")
        .args([action, vm_id])
        .output()
        .map_err(|e| rmrf_core::RmrfError::Msg(format!("qm {action}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(rmrf_core::RmrfError::Msg(format!("qm {action}: {stderr}")));
    }
    Ok(())
}

// ── LibvirtLifecycle ────────────────────────────────────────────

pub struct LibvirtLifecycle;

impl VmLifecycle for LibvirtLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_virsh("suspend", vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_virsh("resume", vm_id)
    }
}

fn run_virsh(action: &str, vm_id: &str) -> rmrf_core::Result<()> {
    let output = Command::new("virsh")
        .args([action, vm_id])
        .output()
        .map_err(|e| rmrf_core::RmrfError::Msg(format!("virsh {action}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(rmrf_core::RmrfError::Msg(format!("virsh {action}: {stderr}")));
    }
    Ok(())
}

// ── VmSource ────────────────────────────────────────────────────

pub struct VmSource<L: VmLifecycle> {
    pub(crate) source: FileSource,
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

        let source = FileSource::new(path).map_err(|e| {
            let _ = lifecycle.resume(vm_id);
            rmrf_core::RmrfError::Io(e)
        })?;

        let mut vm_source = Self {
            source,
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
    use std::io::Read;
    use std::rc::Rc;

    struct MockLifecycle {
        suspended: Rc<RefCell<bool>>,
        resume_calls: Rc<RefCell<u32>>,
        should_fail_suspend: bool,
    }

    impl MockLifecycle {
        fn new(suspended: Rc<RefCell<bool>>, resume_calls: Rc<RefCell<u32>>) -> Self {
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
            SourceType::VmDisk { format, .. } => assert_eq!(*format, DiskFormat::Qcow2),
            _ => panic!("expected VmDisk"),
        }
        let _ = std::fs::remove_file(&path);
    }
}