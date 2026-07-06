mod backup;
mod restore;
mod commands;
use clap::{Parser, Subcommand, ValueEnum};
use rmrf_core::DiskFormat;
use rmrf_source::{
    LibvirtLifecycle, NoopLifecycle, ProxmoxLifecycle, ProxmoxLxcLifecycle, VmLifecycle,
};
use std::fmt;
use std::path::PathBuf;

const DEFAULT_STORE: &str = "./rmrf-store";

#[derive(ValueEnum, Clone)]
enum Hypervisor {
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

#[derive(ValueEnum, Clone)]
enum DiskFormatArg {
    Auto,
    Raw,
    Qcow2,
}

#[derive(Parser)]
#[command(
    name = "rmrf",
    about = "RMRF — chunk-based deduplicated backups with FastCDC + BLAKE3 + zstd",
    long_about = "RMRF — chunk-based deduplicated backups\n\n\
        Features: content-defined chunking (FastCDC), BLAKE3 hashing, \
        zstd compression, cross-run deduplication, and VM disk backup \
        (Proxmox QEMU/LXC, Libvirt)."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a backup of a file, directory, or VM disk
    Backup {
        /// Path to the file, directory, or VM disk to backup (auto-detected with --vm if omitted)
        path: Option<PathBuf>,
        /// Store directory (where chunks, manifests, and index live)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        /// Delete manifests older than N days after backup, then run GC
        #[arg(long)]
        retain: Option<u64>,
        /// VM ID for VM/LXC disk backup (auto-detects qm vs pct)
        #[arg(long)]
        vm: Option<String>,
        /// Disk selector (e.g. scsi1, virtio0, rootfs, mp0). Default: first disk
        #[arg(long)]
        disk: Option<String>,
        /// Disk name for VM metadata (default: basename of path)
        #[arg(long)]
        disk_name: Option<String>,
        /// Disk format: auto (detect from extension), raw, or qcow2
        #[arg(long, default_value = "auto")]
        format: DiskFormatArg,
        /// Skip VM suspend/resume (for already-snapshotted disks)
        #[arg(long)]
        no_suspend: bool,
        /// Hypervisor type: auto, proxmox, proxmox-lxc, libvirt, noop
        #[arg(long, default_value = "auto")]
        hypervisor: Hypervisor,
    },
    /// Restore a file from a backup manifest
    Restore {
        /// Path to the manifest file (.bincode)
        manifest: PathBuf,
        /// Store directory (where chunks live)
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        /// Output path for the restored file (default: manifest basename in CWD)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Overwrite existing output file (safety: restore refuses without this flag)
        #[arg(long)]
        force: bool,
    },
    /// List all backups in the store
    List {
        /// Store directory
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Show detailed info for a specific backup
    Info {
        /// Path to the manifest file (.bincode)
        manifest: PathBuf,
        /// Store directory
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Verify a backup (check all chunks exist on disk)
    Verify {
        /// Path to the manifest file (.bincode)
        manifest: PathBuf,
        /// Store directory
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    /// Garbage collect orphan chunks (not referenced by any manifest)
    Gc {
        /// Store directory
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        /// Dry run: report what would be deleted without deleting
        #[arg(long)]
        dry_run: bool,
    },
    /// Clean test artifacts from the current directory
    Clean,
}

fn main() {
    let cli = Cli::parse();
    let ok = match cli.command {
        Command::Backup {
            path,
            store,
            retain,
            vm,
            disk,
            disk_name,
            format,
            no_suspend,
            hypervisor,
        } => handle_backup(
            path.as_deref(),
            &store,
            retain,
            vm,
            disk.as_deref(),
            disk_name,
            format,
            no_suspend,
            hypervisor,
        ),
        Command::Restore {
            manifest,
            store,
            output,
            force,
        } => restore::restore(&manifest, &store, output.as_deref(), force),
        Command::List { store } => commands::list(&store),
        Command::Info { manifest, store } => commands::info(&manifest, &store),
        Command::Verify { manifest, store } => commands::verify(&manifest, &store),
        Command::Gc { store, dry_run } => commands::gc(&store, dry_run),
        Command::Clean => commands::clean(),
    };
    if !ok {
        std::process::exit(1);
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_backup(
    path: Option<&std::path::Path>,
    store: &std::path::Path,
    retain: Option<u64>,
    vm: Option<String>,
    disk: Option<&str>,
    disk_name: Option<String>,
    format: DiskFormatArg,
    no_suspend: bool,
    hypervisor: Hypervisor,
) -> bool {
    let resolved_path;
    let path = match (path, &vm) {
        (Some(p), _) => p,
        (None, Some(vm_id)) => match detect_disk_path(vm_id, disk) {
            Ok((p, name)) => {
                resolved_path = p;
                if disk_name.is_none() {
                    eprintln!("Auto-detected disk: {name}");
                }
                &resolved_path
            }
            Err(e) => {
                eprintln!("Error auto-detecting disk for VM {vm_id}: {e}");
                return false;
            }
        },
        (None, None) => {
            eprintln!("Error: <PATH> is required unless --vm is specified");
            return false;
        }
    };

    let disk_format = parse_disk_format(&format, path);
    let lifecycle = if no_suspend {
        Box::new(NoopLifecycle) as Box<dyn VmLifecycle>
    } else {
        build_vm_lifecycle(vm.as_deref(), &hypervisor)
    };

    let ok = if let Some(vm_id) = vm {
        let disk_label = disk_name
            .as_deref()
            .unwrap_or_else(|| path.file_name().and_then(|s| s.to_str()).unwrap_or("disk"));
        backup::backup_vm(path, store, &vm_id, disk_label, disk_format, lifecycle)
    } else {
        backup::backup(path, store)
    };

    if !ok {
        return false;
    }

    if let Some(days) = retain {
        commands::run_retention_and_gc(store, days)
    } else {
        true
    }
}

fn parse_disk_format(arg: &DiskFormatArg, path: &std::path::Path) -> DiskFormat {
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

fn detect_hypervisor(vm_id: &str) -> Hypervisor {
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

fn run_cmd(cmd: &str, args: &[&str]) -> bool {
    std::process::Command::new(cmd)
        .args(args)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

const QM_DISK_PREFIXES: &[&str] = &["scsi", "virtio", "ide", "sata", "efidisk", "tpmstate"];
const PCT_DISK_PREFIXES: &[&str] = &["rootfs", "mp"];

fn detect_disk_path(
    vm_id: &str,
    disk_spec: Option<&str>,
) -> Result<(PathBuf, String), String> {
    if let Some(result) = try_qm_config(vm_id, disk_spec)? {
        return Ok(result);
    }
    if let Some(result) = try_pct_config(vm_id, disk_spec)? {
        return Ok(result);
    }
    Err("no disk found in VM config (tried qm + pct)".to_string())
}

fn try_qm_config(
    vm_id: &str,
    disk_spec: Option<&str>,
) -> Result<Option<(PathBuf, String)>, String> {
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

fn try_pct_config(
    vm_id: &str,
    disk_spec: Option<&str>,
) -> Result<Option<(PathBuf, String)>, String> {
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

fn find_disk_from_config(
    config: &str,
    prefixes: &[&str],
    disk_spec: Option<&str>,
) -> Result<Option<(PathBuf, String)>, String> {
    for line in config.lines() {
        let line = line.trim();
        let Some((prefix, rest)) = line.split_once(':') else {
            continue;
        };
        let prefix = prefix.trim();
        if !prefixes.iter().any(|p| prefix.starts_with(p)) {
            continue;
        }
        if let Some(spec) = disk_spec {
            if prefix != spec {
                continue;
            }
        }
        let rest = rest.trim();
        let volume = rest.split(',').next().unwrap_or(rest).trim();
        if volume.is_empty() || volume == "none" {
            continue;
        }
        return resolve_pvesm_path(volume);
    }
    Ok(None)
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

fn build_vm_lifecycle(vm_id: Option<&str>, hypervisor: &Hypervisor) -> Box<dyn VmLifecycle> {
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
