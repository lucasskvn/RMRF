mod backup;
mod commands;
mod config;
mod disk_detect;
mod hypervisor;
mod lock;
mod pipeline;
mod restore;

use clap::{Parser, Subcommand};
use config::RmrfConfig;
use disk_detect::{parse_disk_format, DiskFormatArg};
use hypervisor::Hypervisor;
use lock::{check_stale_sentinels, store_sentinel_path, try_lock_store, unlock_store};
use std::path::PathBuf;

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
    #[arg(long, help = "Path to config file (default: ~/.rmrf.toml)")]
    config: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Backup {
        path: Option<PathBuf>,
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long)]
        retain: Option<u64>,
        #[arg(long)]
        vm: Option<String>,
        #[arg(long)]
        disk: Option<String>,
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
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    List {
        #[arg(short, long)]
        store: Option<PathBuf>,
    },
    Info {
        manifest: PathBuf,
        #[arg(short, long)]
        store: Option<PathBuf>,
    },
    Verify {
        manifest: PathBuf,
        #[arg(short, long)]
        store: Option<PathBuf>,
    },
    Gc {
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let cfg = cli.config.as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok()
            .and_then(|s| toml::from_str(&s).ok()))
        .unwrap_or_else(config::load_config);

    let store = match &cli.command {
        Command::Backup { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Restore { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::List { store } => store.as_deref().unwrap_or(&cfg.store),
        Command::Info { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Verify { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Gc { store, .. } => store.as_deref().unwrap_or(&cfg.store),
    };
    check_stale_sentinels(store);

    let ok = dispatch_command(cli.command, &cfg);
    if !ok {
        std::process::exit(1);
    }
}

fn dispatch_command(command: Command, cfg: &RmrfConfig) -> bool {
    match command {
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
            store.as_deref().unwrap_or(&cfg.store),
            retain,
            vm,
            disk.as_deref(),
            disk_name,
            format,
            no_suspend,
            hypervisor,
            cfg,
        ),
        Command::Restore {
            manifest,
            store,
            output,
            force,
        } => restore::restore(&manifest, store.as_deref().unwrap_or(&cfg.store), output.as_deref(), force),
        Command::List { store } => commands::list(store.as_deref().unwrap_or(&cfg.store)),
        Command::Info { manifest, store } => commands::info(&manifest, store.as_deref().unwrap_or(&cfg.store)),
        Command::Verify { manifest, store } => commands::verify(&manifest, store.as_deref().unwrap_or(&cfg.store)),
        Command::Gc { store, dry_run } => {
            let store = store.as_deref().unwrap_or(&cfg.store);
            if !try_lock_store(store) {
                false
            } else {
                let ok = commands::gc(store, dry_run);
                unlock_store(store);
                ok
            }
        }
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
    cfg: &RmrfConfig,
) -> bool {
    let resolved_path;
    let path = match (path, &vm) {
        (Some(p), _) => p,
        (None, Some(vm_id)) => match disk_detect::detect_disk_path(vm_id, disk) {
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
        Box::new(rmrf_source::NoopLifecycle) as Box<dyn rmrf_source::VmLifecycle>
    } else {
        hypervisor::build_vm_lifecycle(vm.as_deref(), &hypervisor)
    };

    if !try_lock_store(store) {
        return false;
    }
    let ok = if let Some(vm_id) = &vm {
        let disk_label = disk_name
            .as_deref()
            .unwrap_or_else(|| path.file_name().and_then(|s| s.to_str()).unwrap_or("disk"));
        let sentinel = store_sentinel_path(store, vm_id);
        std::fs::write(&sentinel, chrono::Utc::now().to_rfc3339()).ok();
        let result = backup::backup_vm(path, store, vm_id, disk_label, disk_format, lifecycle, cfg);
        std::fs::remove_file(&sentinel).ok();
        result
    } else {
        backup::backup(path, store, cfg)
    };

    if !ok {
        unlock_store(store);
        return false;
    }

    let ok = if let Some(days) = retain {
        commands::run_retention_and_gc(store, days)
    } else {
        true
    };
    unlock_store(store);
    ok
}
