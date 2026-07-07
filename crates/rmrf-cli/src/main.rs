mod backup;
mod commands;
mod config;
mod cron;
mod daemon;
mod disk_detect;
mod hypervisor;
mod lock;
mod pipeline;
mod restore;

use clap::{Parser, Subcommand};
use config::RmrfConfig;
use disk_detect::{parse_disk_format, DiskFormatArg};
use hypervisor::{validate_vm_id, Hypervisor};
use lock::{check_stale_sentinels, store_sentinel_path, try_lock_store, unlock_store};
use std::path::{Path, PathBuf};

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
        #[arg(long)]
        encrypt: Option<String>,
        #[arg(long)]
        live: bool,
        #[arg(long)]
        qmp_socket: Option<PathBuf>,
    },
    Restore {
        manifest: PathBuf,
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        force: bool,
        #[arg(long, default_value = "4")]
        jobs: usize,
        #[arg(long)]
        identity: Option<String>,
    },
    List {
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long, help = "Show only backups after this date (YYYYMMDD)")]
        since: Option<String>,
        #[arg(long, help = "Show only backups before this date (YYYYMMDD)")]
        before: Option<String>,
        #[arg(long, help = "Filter by source path (substring match)")]
        source: Option<String>,
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
        #[arg(long)]
        rehash: bool,
        #[arg(long)]
        identity: Option<String>,
    },
    Gc {
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    Cron,
    Daemon {
        #[arg(long)]
        once: bool,
        #[arg(long)]
        job: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let cfg = if let Some(p) = cli.config.as_ref() {
        match config::load_config_file(p) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    } else {
        config::load_config()
    };

    let store = store_for_command(&cli.command, &cfg);
    check_stale_sentinels(store);

    let ok = dispatch_command(cli.command, &cfg);
    if !ok {
        std::process::exit(1);
    }
}

fn store_for_command<'a>(command: &'a Command, cfg: &'a RmrfConfig) -> &'a Path {
    match command {
        Command::Backup { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Restore { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::List { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Info { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Verify { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Gc { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Cron | Command::Daemon { .. } => &cfg.store,
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_backup(
    path: Option<PathBuf>,
    store: Option<PathBuf>,
    retain: Option<u64>,
    vm: Option<String>,
    disk: Option<String>,
    disk_name: Option<String>,
    format: disk_detect::DiskFormatArg,
    no_suspend: bool,
    hypervisor: hypervisor::Hypervisor,
    cfg: &RmrfConfig,
    encrypt: Option<String>,
    live: bool,
    qmp_socket: Option<PathBuf>,
) -> bool {
    handle_backup(
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
        encrypt.as_deref(),
        live,
        qmp_socket.as_deref(),
    )
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
            encrypt,
            live,
            qmp_socket,
        } => dispatch_backup(
            path, store, retain, vm, disk, disk_name, format,
            no_suspend, hypervisor, cfg, encrypt, live, qmp_socket,
        ),
        Command::Restore {
            manifest,
            store,
            output,
            force,
            jobs,
            identity,
        } => restore::restore(
            &manifest,
            store.as_deref().unwrap_or(&cfg.store),
            output.as_deref(),
            force,
            jobs,
            identity.as_deref(),
        ),
        Command::List {
            store,
            since,
            before,
            source,
        } => commands::list(
            store.as_deref().unwrap_or(&cfg.store),
            since.as_deref(),
            before.as_deref(),
            source.as_deref(),
        ),
        Command::Info { manifest, store } => {
            commands::info(&manifest, store.as_deref().unwrap_or(&cfg.store))
        }
        Command::Verify {
            manifest,
            store,
            rehash,
            identity,
        } => commands::verify(
            &manifest,
            store.as_deref().unwrap_or(&cfg.store),
            rehash,
            identity.as_deref(),
        ),
        Command::Gc { store, dry_run } => {
            handle_gc(store.as_deref().unwrap_or(&cfg.store), dry_run)
        }
        Command::Cron => handle_cron(cfg),
        Command::Daemon { once, job } => daemon::run_daemon(once, job.as_deref(), cfg),
    }
}

fn handle_gc(store: &Path, dry_run: bool) -> bool {
    if !try_lock_store(store) {
        return false;
    }
    let ok = commands::gc(store, dry_run);
    unlock_store(store);
    ok
}

fn handle_cron(cfg: &RmrfConfig) -> bool {
    let binary = std::env::current_exe().unwrap_or_else(|_| "rmrf".into());
    let out = cron::generate_crontab(&cfg.jobs, &binary);
    if out.is_empty() {
        println!("No scheduled jobs configured in ~/.rmrf.toml");
    } else {
        print!("{out}");
    }
    true
}

fn resolve_backup_path(path: Option<&Path>, vm: &Option<String>, disk: Option<&str>, disk_name: &Option<String>) -> Result<(std::path::PathBuf, String), String> {
    match (path, vm) {
        (Some(p), _) => Ok((p.to_path_buf(), String::new())),
        (None, Some(vm_id)) => {
            disk_detect::detect_disk_path(vm_id, disk).map(|(p, name)| {
                if disk_name.is_none() {
                    eprintln!("Auto-detected disk: {name}");
                }
                (p, name)
            })
        }
        (None, None) => Err("<PATH> is required unless --vm is specified".into()),
    }
}

fn build_lifecycle(vm: &Option<String>, no_suspend: bool, hypervisor: &Hypervisor) -> Box<dyn rmrf_source::VmLifecycle> {
    if no_suspend {
        Box::new(rmrf_source::NoopLifecycle)
    } else {
        hypervisor::build_vm_lifecycle(vm.as_deref(), hypervisor)
    }
}

#[allow(clippy::too_many_arguments)]
fn run_backup_with_vm(
    path: &Path,
    store: &Path,
    vm_id: &str,
    disk_name: &Option<String>,
    disk_format: rmrf_core::DiskFormat,
    lifecycle: Box<dyn rmrf_source::VmLifecycle>,
    cfg: &RmrfConfig,
    encrypt: Option<&str>,
) -> bool {
    let disk_label = disk_name
        .as_deref()
        .unwrap_or_else(|| path.file_name().and_then(|s| s.to_str()).unwrap_or("disk"));
    let sentinel = store_sentinel_path(store, vm_id);
    std::fs::write(&sentinel, chrono::Utc::now().to_rfc3339()).ok();
    let result = backup::backup_vm(path, store, vm_id, disk_label, disk_format, lifecycle, cfg, encrypt);
    std::fs::remove_file(&sentinel).ok();
    result
}

#[allow(clippy::too_many_arguments)]
fn run_backup_with_lock(path: &Path, store: &Path, vm: &Option<String>, disk_name: &Option<String>, disk_format: rmrf_core::DiskFormat, lifecycle: Box<dyn rmrf_source::VmLifecycle>, cfg: &RmrfConfig, encrypt: Option<&str>, retain: Option<u64>) -> bool {
    if !try_lock_store(store) {
        return false;
    }
    let ok = if let Some(vm_id) = vm {
        run_backup_with_vm(path, store, vm_id, disk_name, disk_format, lifecycle, cfg, encrypt)
    } else {
        backup::backup(path, store, cfg, encrypt)
    };
    if !ok {
        unlock_store(store);
        return false;
    }
    if let Some(days) = retain {
        let ok = commands::run_retention_and_gc(store, days);
        unlock_store(store);
        ok
    } else {
        unlock_store(store);
        true
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
    encrypt: Option<&str>,
    live: bool,
    _qmp_socket: Option<&std::path::Path>,
) -> bool {
    if live {
        eprintln!(
            "Error: --live (QMP live backup) is disabled. \
             blockdev-snapshot-sync creates an overlay disk with no merge-back \
             path, which can corrupt the VM disk. Use suspend-based backup \
             (drop --live) instead."
        );
        return false;
    }
    if let Some(vm_id) = vm.as_deref() {
        if let Err(e) = validate_vm_id(vm_id) {
            eprintln!("Error: {e}");
            return false;
        }
    }

    let (resolved_path, _) = match resolve_backup_path(path, &vm, disk, &disk_name) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            return false;
        }
    };

    let disk_format = parse_disk_format(&format, &resolved_path);
    let lifecycle = build_lifecycle(&vm, no_suspend, &hypervisor);
    run_backup_with_lock(&resolved_path, store, &vm, &disk_name, disk_format, lifecycle, cfg, encrypt, retain)
}
