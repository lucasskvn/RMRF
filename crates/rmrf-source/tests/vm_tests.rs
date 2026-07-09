use rmrf_core::{DiskFormat, Source, SourceType};
use rmrf_source::{NoopLifecycle, ProxmoxLxcLifecycle, VmLifecycle, VmSource, VmSourceOptions};
use std::cell::RefCell;
use std::io::{Read, Write};
use std::rc::Rc;

struct MockLifecycle {
    suspended: Rc<RefCell<bool>>,
    resume_calls: Rc<RefCell<u32>>,
    should_fail_suspend: bool,
}

impl MockLifecycle {
    fn new() -> Self {
        Self {
            suspended: Rc::new(RefCell::new(false)),
            resume_calls: Rc::new(RefCell::new(0)),
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
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content).unwrap();
    path
}

#[test]
fn noop_lifecycle_does_nothing() {
    let noop = NoopLifecycle;
    assert!(noop.suspend("100").is_ok());
    assert!(noop.resume("100").is_ok());
}

#[test]
fn vm_source_opens_with_noop_lifecycle() {
    let path = create_test_file("rmrf_test_vm_open", b"vm data");
    let opts = VmSourceOptions { path: &path, vm_id: "100", disk_name: "disk0", format: DiskFormat::Raw };
    let vm = VmSource::open(NoopLifecycle, &opts);
    assert!(vm.is_ok());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn vm_source_reads_content() {
    let path = create_test_file("rmrf_test_vm_read", b"hello vm");
    let opts = VmSourceOptions { path: &path, vm_id: "200", disk_name: "vda", format: DiskFormat::Raw };
    let mut vm = VmSource::open(NoopLifecycle, &opts).unwrap();
    let mut buf = [0u8; 64];
    let n = vm.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"hello vm");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn vm_source_drop_calls_resume() {
    let path = create_test_file("rmrf_test_vm_drop", b"drop test");
    let lifecycle = MockLifecycle::new();
    let resume_calls = Rc::clone(&lifecycle.resume_calls);
    {
        let opts = VmSourceOptions { path: &path, vm_id: "400", disk_name: "sda", format: DiskFormat::Raw };
        let _vm = VmSource::open(lifecycle, &opts).unwrap();
    }
    assert_eq!(*resume_calls.borrow(), 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn vm_source_suspend_failure_propagates() {
    let path = create_test_file("rmrf_test_vm_sfail", b"suspend fail");
    let opts = VmSourceOptions { path: &path, vm_id: "500", disk_name: "sda", format: DiskFormat::Raw };
    let result = VmSource::open(MockLifecycle::failing(), &opts);
    assert!(result.is_err());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn vm_source_open_failure_after_suspend_resumes() {
    use std::os::unix::fs::PermissionsExt;

    let path = std::env::temp_dir().join("rmrf_test_vm_noread_perm");
    std::fs::write(&path, b"test").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&path, perms).unwrap();
    let is_root = std::fs::File::open(&path).is_ok();
    let _ = std::fs::remove_file(&path);
    if is_root {
        eprintln!("Skipping test: root bypasses file permissions");
        return;
    }

    let lifecycle = MockLifecycle::new();
    let resume_calls = Rc::clone(&lifecycle.resume_calls);
    let suspended = Rc::clone(&lifecycle.suspended);

    let path = std::env::temp_dir().join("rmrf_test_vm_noread");
    std::fs::write(&path, b"test").unwrap();
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o000);
    std::fs::set_permissions(&path, perms).unwrap();

    let opts = VmSourceOptions { path: &path, vm_id: "600", disk_name: "sda", format: DiskFormat::Raw };
    let result = VmSource::open(lifecycle, &opts);
    assert!(result.is_err());
    assert!(!*suspended.borrow(), "should be resumed after open failure");
    assert_eq!(*resume_calls.borrow(), 1);

    let _ = std::fs::remove_file(&path);
}

#[test]
fn vm_source_file_not_found_before_suspend() {
    let lifecycle = MockLifecycle::new();
    let suspended = Rc::clone(&lifecycle.suspended);
    let resume_calls = Rc::clone(&lifecycle.resume_calls);
    let opts = VmSourceOptions { path: std::path::Path::new("/tmp/nonexistent_vm_disk_888"), vm_id: "700", disk_name: "nvme0", format: DiskFormat::Qcow2 };
    let result = VmSource::open(lifecycle, &opts);
    assert!(result.is_err());
    assert!(!*suspended.borrow());
    assert_eq!(*resume_calls.borrow(), 1);
}

#[test]
fn vm_source_qcow2_format_metadata() {
    let path = create_test_file("rmrf_test_vm_qcow2", b"qcow2 data");
    let opts = VmSourceOptions { path: &path, vm_id: "800", disk_name: "nvme0", format: DiskFormat::Qcow2 };
    let vm = VmSource::open(NoopLifecycle, &opts).unwrap();
    let meta = vm.metadata();
    assert_eq!(
        meta.source_type,
        SourceType::VmDisk {
            vm_id: "800".to_string(),
            disk_name: "nvme0".to_string(),
            format: DiskFormat::Qcow2,
        }
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn lxc_lifecycle_conforms_to_trait() {
    let lifecycle = ProxmoxLxcLifecycle;
    let result = lifecycle.suspend("9999");
    assert!(
        result.is_err(),
        "pct suspend on non-existent CT should fail"
    );
    let result = lifecycle.resume("9999");
    assert!(result.is_err(), "pct resume on non-existent CT should fail");
}
