use crate::FileSource;
use rmrf_core::{DiskFormat, Source, SourceMetadata, SourceType};
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

#[allow(dead_code)]
pub struct QmpLifecycle {
    socket_path: std::path::PathBuf,
    device: String,
}

impl QmpLifecycle {
    pub fn new(socket_path: &Path, device: &str) -> Self {
        QmpLifecycle {
            socket_path: socket_path.to_path_buf(),
            device: device.to_string(),
        }
    }
}

impl VmLifecycle for QmpLifecycle {
    fn suspend(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Err(rmrf_core::RmrfError::Msg(
            "live QMP backup is disabled: use suspend-based backup (drop --live) instead".into(),
        ))
    }
    fn resume(&self, _vm_id: &str) -> rmrf_core::Result<()> {
        Err(rmrf_core::RmrfError::Msg(
            "QMP live backup resume is not implemented".into(),
        ))
    }
}

fn run_external(cmd: &str, action: &str, vm_id: &str) -> rmrf_core::Result<()> {
    let output = Command::new(cmd)
        .args([action, vm_id])
        .output()
        .map_err(|e| rmrf_core::RmrfError::Msg(format!("{cmd} {action}: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(rmrf_core::RmrfError::Msg(format!(
            "{cmd} {action}: {stderr}"
        )));
    }
    Ok(())
}

pub struct VmSourceOptions<'a> {
    pub path: &'a Path,
    pub vm_id: &'a str,
    pub disk_name: &'a str,
    pub format: DiskFormat,
}

pub struct VmSource<L: VmLifecycle> {
    pub(crate) source: FileSource,
    lifecycle: L,
    vm_id: String,
    suspended: bool,
}

impl<L: VmLifecycle> VmSource<L> {

    pub fn open(lifecycle: L, opts: &VmSourceOptions) -> rmrf_core::Result<Self> {
        lifecycle.suspend(opts.vm_id)?;

        let source_path = match lifecycle.snapshot_path() {
            Some(snap) => snap,
            None => opts.path.to_path_buf(),
        };

        if !source_path.exists() {
            if let Err(e) = lifecycle.resume(opts.vm_id) {
                eprintln!("Warning: failed to resume VM {}: {e}", opts.vm_id);
            }
            return Err(rmrf_core::RmrfError::Msg(format!(
                "VM disk not found: {}",
                source_path.display()
            )));
        }

        let source = FileSource::new(&source_path).map_err(|e| {
            if let Err(re) = lifecycle.resume(opts.vm_id) {
                eprintln!("Warning: failed to resume VM {}: {re}", opts.vm_id);
            }
            rmrf_core::RmrfError::Io(e)
        })?;

        let mut vm_source = Self {
            source,
            lifecycle,
            vm_id: opts.vm_id.to_string(),
            suspended: true,
        };

        vm_source.source.metadata.source_type = SourceType::VmDisk {
            vm_id: opts.vm_id.to_string(),
            disk_name: opts.disk_name.to_string(),
            format: opts.format,
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
            if let Err(e) = self.lifecycle.resume(&self.vm_id) {
                eprintln!("Warning: failed to resume VM {}: {e}", self.vm_id);
            }
            self.suspended = false;
        }
    }
}
