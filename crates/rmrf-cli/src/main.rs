mod backup;
mod restore;
mod commands;
use clap::{Parser, Subcommand, ValueEnum};
use rmrf_core::DiskFormat;
use rmrf_source::{LibvirtLifecycle, NoopLifecycle, ProxmoxLifecycle, VmLifecycle};
use std::fmt;
use std::path::PathBuf;

const DEFAULT_STORE: &str = "./rmrf-store";

#[derive(ValueEnum, Clone)]
enum Hypervisor {
    Auto,
    Proxmox,
    Libvirt,
    Noop,
}

impl fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hypervisor::Auto => write!(f, "auto"),
            Hypervisor::Proxmox => write!(f, "proxmox"),
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
    about = "RMRF backup tool — chunk-based deduplicated backups"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Backup {
        path: PathBuf,
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        #[arg(long)]
        retain: Option<u64>,
        #[arg(long)]
        vm: Option<String>,
        #[arg(long)]
        disk_name: Option<String>,
        #[arg(long, default_value = "auto")]
        format: DiskFormatArg,
        #[arg(long)]
        no_suspend: bool,
        #[arg(long, default_value = "auto")]
        hypervisor: Hypervisor,
    },
    Restore {
        manifest: PathBuf,
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    List {
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    Info {
        manifest: PathBuf,
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    Verify {
        manifest: PathBuf,
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
    },
    Gc {
        #[arg(short, long, default_value = DEFAULT_STORE)]
        store: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    Clean,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Backup {
            path,
            store,
            retain,
            vm,
            disk_name,
            format,
            no_suspend,
            hypervisor,
        } => {
            let format = parse_disk_format(&format, &path);
            let lifecycle = if no_suspend {
                Box::new(NoopLifecycle) as Box<dyn VmLifecycle>
            } else {
                build_vm_lifecycle(vm.as_deref(), &hypervisor)
            };

            if let Some(vm_id) = vm {
                let disk = disk_name
                    .as_deref()
                    .unwrap_or_else(|| path.file_name().and_then(|s| s.to_str()).unwrap_or("disk"));
                backup::backup_vm(&path, &store, &vm_id, disk, format, lifecycle);
            } else {
                backup::backup(&path, &store);
            }

            if let Some(days) = retain {
                commands::run_retention_and_gc(&store, days);
            }
        }
        Command::Restore {
            manifest,
            store,
            output,
        } => restore::restore(&manifest, &store, output.as_deref()),
        Command::List { store } => commands::list(&store),
        Command::Info { manifest, store } => commands::info(&manifest, &store),
        Command::Verify { manifest, store } => commands::verify(&manifest, &store),
        Command::Gc { store, dry_run } => commands::gc(&store, dry_run),
        Command::Clean => commands::clean(),
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
    if std::process::Command::new("qm")
        .args(["status", vm_id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Hypervisor::Proxmox;
    }
    if std::process::Command::new("virsh")
        .args(["dominfo", vm_id])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Hypervisor::Libvirt;
    }
    Hypervisor::Noop
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
        Hypervisor::Proxmox => Hypervisor::Proxmox,
        Hypervisor::Libvirt => Hypervisor::Libvirt,
        Hypervisor::Noop => Hypervisor::Noop,
    };
    match hv {
        Hypervisor::Proxmox => Box::new(ProxmoxLifecycle),
        Hypervisor::Libvirt => Box::new(LibvirtLifecycle),
        _ => Box::new(NoopLifecycle),
    }
}
