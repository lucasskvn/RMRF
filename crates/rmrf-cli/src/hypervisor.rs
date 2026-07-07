use clap::ValueEnum;
use rmrf_source::{
    LibvirtLifecycle, NoopLifecycle, ProxmoxLifecycle, ProxmoxLxcLifecycle, VmLifecycle,
};
use std::fmt;

#[derive(ValueEnum, Clone)]
pub enum Hypervisor {
    Auto,
    Proxmox,
    ProxmoxLxc,
    Libvirt,
    Noop,
}

impl fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hypervisor::Auto => write!(f, "auto"),
            Hypervisor::Proxmox => write!(f, "proxmox"),
            Hypervisor::ProxmoxLxc => write!(f, "proxmox-lxc"),
            Hypervisor::Libvirt => write!(f, "libvirt"),
            Hypervisor::Noop => write!(f, "noop"),
        }
    }
}

fn run_cmd(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn detect_hypervisor(vm_id: &str) -> Hypervisor {
    if run_cmd("qm", &["status", vm_id]) {
        return Hypervisor::Proxmox;
    }
    if run_cmd("pct", &["status", vm_id]) {
        return Hypervisor::ProxmoxLxc;
    }
    if run_cmd("virsh", &["dominfo", vm_id]) {
        return Hypervisor::Libvirt;
    }
    Hypervisor::Noop
}

pub fn build_vm_lifecycle(vm_id: Option<&str>, hypervisor: &Hypervisor) -> Box<dyn VmLifecycle> {
    let hv = match hypervisor {
        Hypervisor::Auto => {
            if let Some(id) = vm_id {
                detect_hypervisor(id)
            } else {
                Hypervisor::Noop
            }
        }
        other => other.clone(),
    };
    match hv {
        Hypervisor::Proxmox => Box::new(ProxmoxLifecycle),
        Hypervisor::ProxmoxLxc => Box::new(ProxmoxLxcLifecycle),
        Hypervisor::Libvirt => Box::new(LibvirtLifecycle),
        _ => Box::new(NoopLifecycle),
    }
}
