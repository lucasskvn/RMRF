use clap::ValueEnum;
use rmrf_core::DiskFormat;
use std::path::PathBuf;

#[derive(ValueEnum, Clone)]
pub enum DiskFormatArg {
    Auto,
    Raw,
    Qcow2,
}

pub fn parse_disk_format(arg: &DiskFormatArg, path: &std::path::Path) -> DiskFormat {
    match arg {
        DiskFormatArg::Auto => {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext.eq_ignore_ascii_case("qcow2") {
                    return DiskFormat::Qcow2;
                }
            }
            DiskFormat::Raw
        }
        DiskFormatArg::Raw => DiskFormat::Raw,
        DiskFormatArg::Qcow2 => DiskFormat::Qcow2,
    }
}

const QM_DISK_PREFIXES: &[&str] = &["scsi", "virtio", "ide", "sata", "efidisk", "tpmstate"];
const PCT_DISK_PREFIXES: &[&str] = &["rootfs", "mp"];

pub fn detect_disk_path(vm_id: &str, disk_spec: Option<&str>) -> Result<(PathBuf, String), String> {
    if let Some(result) = try_qm_config(vm_id, disk_spec)? {
        return Ok(result);
    }
    if let Some(result) = try_pct_config(vm_id, disk_spec)? {
        return Ok(result);
    }
    Err("no disk found in VM config (tried qm + pct)".to_string())
}

fn try_qm_config(vm_id: &str,disk_spec: Option<&str>) -> Result<Option<(PathBuf, String)>, String> {
    let output = std::process::Command::new("qm")
        .args(["config", vm_id])
        .output()
        .map_err(|e| format!("qm config: {e}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let config = String::from_utf8_lossy(&output.stdout);
    find_disk_from_config(&config, QM_DISK_PREFIXES, disk_spec)
}

fn try_pct_config(vm_id: &str,disk_spec: Option<&str>) -> Result<Option<(PathBuf, String)>, String> {
    let output = std::process::Command::new("pct")
        .args(["config", vm_id])
        .output()
        .map_err(|e| format!("pct config: {e}"))?;
    if !output.status.success() {
        return Ok(None);
    }
    let config = String::from_utf8_lossy(&output.stdout);
    find_disk_from_config(&config, PCT_DISK_PREFIXES, disk_spec)
}

fn is_auxiliary_disk(name: &str) -> bool {
    name.starts_with("efidisk") || name.starts_with("tpmstate")
}

fn find_disk_from_config(config: &str,prefixes: &[&str],disk_spec: Option<&str>) -> Result<Option<(PathBuf, String)>, String> {
    if let Some(spec) = disk_spec {
        return find_specific_disk(config, prefixes, spec);
    }
    let candidates = collect_disk_candidates(config, prefixes);
    if candidates.is_empty() {
        return Ok(None);
    }
    let data_candidates: Vec<_> = candidates
        .iter()
        .filter(|(name, _)| !is_auxiliary_disk(name))
        .collect();
    if data_candidates.is_empty() {
        return Err(
            "only EFI/TPM disks found in VM config; refusing to back up an EFI \
             disk as the VM's main disk — add a data disk or pass --disk <name>"
                .into(),
        );
    }
    if data_candidates.len() > 1 {
        let names: Vec<&str> = data_candidates.iter().map(|(n, _)| *n).collect();
        eprintln!(
            "Warning: multiple disks found in VM config ({}); selecting '{}'. \
             Pass --disk <name> to back up a specific one.",
            names.join(", "),
            data_candidates[0].0
        );
    }
    resolve_pvesm_path(data_candidates[0].1)
}

fn find_specific_disk(config: &str,prefixes: &[&str],disk_spec: &str) -> Result<Option<(PathBuf, String)>, String> {
    for line in config.lines() {
        let line = line.trim();
        let Some((prefix, rest)) = line.split_once(':') else {
            continue;
        };
        let prefix = prefix.trim();
        if !prefixes.iter().any(|p| prefix.starts_with(p)) {
            continue;
        }
        if prefix != disk_spec {
            continue;
        }
        let volume = rest.trim().split(',').next().unwrap_or(rest).trim();
        if volume.is_empty() || volume == "none" {
            continue;
        }
        return resolve_pvesm_path(volume);
    }
    Ok(None)
}

fn collect_disk_candidates<'a>(config: &'a str, prefixes: &[&str]) -> Vec<(&'a str, &'a str)> {
    let mut candidates = Vec::new();
    for line in config.lines() {
        let line = line.trim();
        let Some((prefix, rest)) = line.split_once(':') else {
            continue;
        };
        let prefix = prefix.trim();
        if !prefixes.iter().any(|p| prefix.starts_with(p)) {
            continue;
        }
        let volume = rest.trim().split(',').next().unwrap_or(rest).trim();
        if volume.is_empty() || volume == "none" {
            continue;
        }
        candidates.push((prefix, volume));
    }
    candidates
}

fn resolve_pvesm_path(volume: &str) -> Result<Option<(PathBuf, String)>, String> {
    let output = std::process::Command::new("pvesm")
        .args(["path", volume])
        .output()
        .map_err(|e| format!("pvesm path: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("pvesm path {volume}: {stderr}"));
    }
    let path_str = String::from_utf8_lossy(&output.stdout);
    let path = PathBuf::from(path_str.trim());
    let disk_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "disk".to_string());
    Ok(Some((path, disk_name)))
}
