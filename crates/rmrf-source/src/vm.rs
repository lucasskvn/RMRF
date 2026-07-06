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