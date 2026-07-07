use crate::qmp::QmpClient;
use crate::FileSource;
use rmrf_core::{DiskFormat, Source, SourceMetadata, SourceType};
use std::cell::RefCell;
use std::io;
use std::path::Path;
use std::process::Command;

pub trait VmLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()>;
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()>;
    fn snapshot_path(&self) -> Option<std::path::PathBuf> {
        None
    }
}

impl VmLifecycle for Box<dyn VmLifecycle> {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        (**self).suspend(vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        (**self).resume(vm_id)
    }
    fn snapshot_path(&self) -> Option<std::path::PathBuf> {
        (**self).snapshot_path()
    }
}

pub struct NoopLifecycle;

impl VmLifecycle for NoopLifecycle {
    fn suspend(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }
    fn resume(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }
}

pub struct ProxmoxLifecycle;

impl VmLifecycle for ProxmoxLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_external("qm", "suspend", vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_external("qm", "resume", vm_id)
    }
}

pub struct ProxmoxLxcLifecycle;

impl VmLifecycle for ProxmoxLxcLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_external("pct", "suspend", vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_external("pct", "resume", vm_id)
    }
}

pub struct LibvirtLifecycle;

impl VmLifecycle for LibvirtLifecycle {
    fn suspend(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_external("virsh", "suspend", vm_id)
    }
    fn resume(&self, vm_id: &str) -> rmrf_core::Result<()> {
        run_external("virsh", "resume", vm_id)
    }
}

pub struct QmpLifecycle {
    socket_path: std::path::PathBuf,
    device: String,
    snapshot_path: RefCell<Option<std::path::PathBuf>>,
}

impl QmpLifecycle {
    pub fn new(socket_path: &Path, device: &str) -> Self {
        QmpLifecycle {
            socket_path: socket_path.to_path_buf(),
            device: device.to_string(),
            snapshot_path: RefCell::new(None),
        }
    }
}

impl VmLifecycle for QmpLifecycle {
    fn suspend(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        let mut client = QmpClient::connect(&self.socket_path)?;
        let snap = std::path::PathBuf::from(format!("/tmp/rmrf-qmp-{}.snap", self.device));
        client.create_snapshot(&self.device, &snap, "qcow2")?;
        *self.snapshot_path.borrow_mut() = Some(snap);
        Ok(())
    }
    fn resume(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Ok(())
    }
    fn snapshot_path(&self) -> Option<std::path::PathBuf> {
        self.snapshot_path.borrow().clone()
    }
}

fn run_external(cmd: &str, action: &str, vm_id: &str) -> rmrf_core::Result<()> {
    let output = Command::new(cmd)
        .args([action, vm_id])
        .output()
        .map_err(|e| rmrf_core::RmrfError::Msg(format!("{cmd} {action}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(rmrf_core::RmrfError::Msg(format!("{cmd} {action}: {stderr}")));
    }
    Ok(())
}

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
        lifecycle.suspend(vm_id)?;

        let source_path = match lifecycle.snapshot_path() {
            Some(snap) => snap,
            None => path.to_path_buf(),
        };

        if !source_path.exists() {
            let _ = lifecycle.resume(vm_id);
            return Err(rmrf_core::RmrfError::Msg(format!(
                "VM disk not found: {}",
                source_path.display()
            )));
        }

        let source = FileSource::new(&source_path).map_err(|e| {
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