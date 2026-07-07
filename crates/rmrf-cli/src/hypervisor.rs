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

pub fn validate_vm_id(vm_id: &str) -> Result<(), String> {
    if vm_id.is_empty() {
        return Err("VM id is empty".into());
    }
    if vm_id.len() > 128 {
        return Err("VM id is too long (max 128 characters)".into());
    }
    if vm_id.starts_with('-') {
        return Err(format!("VM id must not start with '-': {vm_id:?}"));
    }
    for c in vm_id.chars() {
        if c.is_control() || c.is_whitespace() || c == '/' || c == '\\' || c == '\0' {
            return Err(format!("VM id contains disallowed character: {vm_id:?}"));
        }
    }
    if vm_id.contains("..") {
        return Err(format!("VM id must not contain '..': {vm_id:?}"));
    }
    if !vm_id.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("VM id must be numeric, got {vm_id:?}"));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::validate_vm_id;

    #[test]
    fn valid_vm_ids_accepted() {
        assert!(validate_vm_id("100").is_ok());
        assert!(validate_vm_id("9999").is_ok());
        assert!(validate_vm_id("0").is_ok());
    }

    #[test]
    fn rejects_empty_and_overlong() {
        assert!(validate_vm_id("").is_err());
        let long = "a".repeat(129);
        assert!(validate_vm_id(&long).is_err());
    }

    #[test]
    fn rejects_leading_dash() {
        assert!(validate_vm_id("-100").is_err());
        assert!(validate_vm_id("--help").is_err());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_vm_id("../etc").is_err());
        assert!(validate_vm_id("a/b").is_err());
        assert!(validate_vm_id("a\\b").is_err());
    }

    #[test]
    fn rejects_whitespace_and_control() {
        assert!(validate_vm_id("100 200").is_err());
        assert!(validate_vm_id("100\n200").is_err());
        assert!(validate_vm_id("100\t200").is_err());
    }
}
