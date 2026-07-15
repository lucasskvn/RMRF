mod backup;
mod commands;
mod config;
mod disk_detect;
mod hypervisor;
mod lock;
mod pipeline;
mod restore;
mod signal;

use clap::{Parser, Subcommand};
use config::RmrfConfig;
use disk_detect::{parse_disk_format, DiskFormatArg};
use hypervisor::{validate_vm_id, Hypervisor};
use lock::{
    check_stale_sentinels, store_sentinel_path, try_lock_store, unlock_store, SentinelCheckResult,
};
use std::path::{Path, PathBuf};

struct BackupOptions {
    path: Option<PathBuf>,
    store: Option<PathBuf>,
    retain: Option<u64>,
    vm: Option<String>,
    disk: Option<String>,
    disk_name: Option<String>,
    format: disk_detect::DiskFormatArg,
    no_suspend: bool,
    hypervisor: hypervisor::Hypervisor,
    encrypt: Option<String>,
    live: bool,
    _qmp_socket: Option<PathBuf>,
}

struct RunBackupOptions<'a> {
    path: &'a Path,
    store: &'a Path,
    vm: &'a Option<String>,
    disk_name: &'a Option<String>,
    disk_format: rmrf_core::DiskFormat,
    cfg: &'a RmrfConfig,
    encrypt: Option<&'a str>,
    retain: Option<u64>,
}

#[derive(Parser)]
#[command(
    name = "rmrf",
    version = env!("CARGO_PKG_VERSION"),
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
    #[command(about = "Backup a file, directory, or VM disk")]
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
    #[command(about = "Restore a backup from a manifest file")]
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
        #[arg(long)]
        vm: Option<String>,
        #[arg(long)]
        disk: Option<String>,
        #[arg(long)]
        no_suspend: bool,
        #[arg(long, default_value = "auto")]
        hypervisor: Hypervisor,
    },
    #[command(about = "List all backups in a store")]
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
    #[command(about = "Show detailed information about a backup")]
    Info {
        manifest: PathBuf,
        #[arg(short, long)]
        store: Option<PathBuf>,
    },
    #[command(about = "Verify all chunks in a backup manifest")]
    Verify {
        manifest: PathBuf,
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long)]
        rehash: bool,
        #[arg(long)]
        identity: Option<String>,
    },
    #[command(about = "Garbage collect orphan chunks in a store")]
    Gc {
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Remove manifests older than N days")]
    Retention {
        #[arg(short, long)]
        store: Option<PathBuf>,
        #[arg(long)]
        days: u64,
        #[arg(long)]
        dry_run: bool,
    },
    #[command(about = "Check for stale VM sentinels and auto-resume suspended VMs")]
    Check {
        #[arg(short, long)]
        store: Option<PathBuf>,
    },
}

fn main() {
    signal::setup_signal_handler();
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
    if !matches!(cli.command, Command::Check { .. }) {
        check_stale_sentinels(store);
    }

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
        Command::Retention { store, .. } => store.as_deref().unwrap_or(&cfg.store),
        Command::Check { store, .. } => store.as_deref().unwrap_or(&cfg.store),
    }
}

fn dispatch_backup(opts: BackupOptions, cfg: &RmrfConfig) -> bool {
    handle_backup(&opts, cfg)
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
            BackupOptions {
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
                _qmp_socket: qmp_socket,
            },
            cfg,
        ),
        Command::Restore {
            manifest,
            store,
            output,
            force,
            jobs,
            identity,
            vm,
            disk,
            no_suspend,
            hypervisor,
        } => {
            let restore_opts = restore::RestoreOptions {
                manifest_path: &manifest,
                store_dir: store.as_deref().unwrap_or(&cfg.store),
                output: output.as_deref(),
                force,
                jobs,
                identity: identity.as_deref(),
                vm: vm.as_deref(),
                disk: disk.as_deref(),
                no_suspend,
                hypervisor: &hypervisor,
            };
            restore::restore(&restore_opts)
        }
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
        Command::Retention {
            store,
            days,
            dry_run,
        } => commands::run_retention_and_gc(store.as_deref().unwrap_or(&cfg.store), days, dry_run),
        Command::Check { store } => {
            let result = check_stale_sentinels(store.as_deref().unwrap_or(&cfg.store));
            matches!(
                result,
                SentinelCheckResult::Clean | SentinelCheckResult::Recovered
            )
        }
    }
}

fn handle_gc(store: &Path, dry_run: bool) -> bool {
    if !store.exists() {
        eprintln!("Error: store directory not found: {}", store.display());
        return false;
    }
    if !try_lock_store(store) {
        return false;
    }
    let ok = commands::gc(store, dry_run);
    unlock_store(store);
    ok
}

fn resolve_backup_path(
    path: Option<&Path>,
    vm: &Option<String>,
    disk: Option<&str>,
    disk_name: &Option<String>,
) -> Result<(std::path::PathBuf, String), String> {
    match (path, vm) {
        (Some(p), _) => Ok((p.to_path_buf(), String::new())),
        (None, Some(vm_id)) => disk_detect::detect_disk_path(vm_id, disk).map(|(p, name)| {
            if disk_name.is_none() {
                eprintln!("Auto-detected disk: {name}");
            }
            (p, name)
        }),
        (None, None) => Err("<PATH> is required unless --vm is specified".into()),
    }
}

fn build_lifecycle(
    vm: &Option<String>,
    no_suspend: bool,
    hypervisor: &Hypervisor,
) -> Box<dyn rmrf_source::VmLifecycle> {
    if no_suspend {
        Box::new(rmrf_source::NoopLifecycle)
    } else {
        hypervisor::build_vm_lifecycle(vm.as_deref(), hypervisor)
    }
}

fn run_backup_with_vm(
    opts: &RunBackupOptions,
    vm_id: &str,
    lifecycle: Box<dyn rmrf_source::VmLifecycle>,
) -> bool {
    let disk_label = opts.disk_name.as_deref().unwrap_or_else(|| {
        opts.path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("disk")
    });
    let sentinel = store_sentinel_path(opts.store, vm_id);
    std::fs::write(&sentinel, chrono::Utc::now().to_rfc3339()).ok();
    let backup_vm_opts = backup::BackupVmOptions {
        path: opts.path,
        store_dir: opts.store,
        vm_id,
        disk_name: disk_label,
        format: opts.disk_format,
        cfg: opts.cfg,
        encrypt: opts.encrypt,
    };
    let result = backup::backup_vm(lifecycle, &backup_vm_opts);
    std::fs::remove_file(&sentinel).ok();
    result
}

fn run_backup_with_lock(
    opts: &RunBackupOptions,
    lifecycle: Box<dyn rmrf_source::VmLifecycle>,
) -> bool {
    if !try_lock_store(opts.store) {
        return false;
    }
    let ok = if let Some(vm_id) = opts.vm {
        run_backup_with_vm(opts, vm_id, lifecycle)
    } else {
        backup::backup(opts.path, opts.store, opts.cfg, opts.encrypt)
    };
    if !ok {
        unlock_store(opts.store);
        return false;
    }
    if let Some(days) = opts.retain {
        let ok = commands::run_retention_and_gc(opts.store, days, false);
        unlock_store(opts.store);
        ok
    } else {
        unlock_store(opts.store);
        true
    }
}

fn handle_backup(opts: &BackupOptions, cfg: &RmrfConfig) -> bool {
    if opts.live {
        eprintln!(
            "Error: --live (QMP live backup) is disabled. \
             blockdev-snapshot-sync creates an overlay disk with no merge-back \
             path, which can corrupt the VM disk. Use suspend-based backup \
             (drop --live) instead."
        );
        return false;
    }
    if let Some(vm_id) = opts.vm.as_deref() {
        if let Err(e) = validate_vm_id(vm_id) {
            eprintln!("Error: {e}");
            return false;
        }
    }

    let (resolved_path, _) = match resolve_backup_path(
        opts.path.as_deref(),
        &opts.vm,
        opts.disk.as_deref(),
        &opts.disk_name,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {e}");
            return false;
        }
    };

    let disk_format = parse_disk_format(&opts.format, &resolved_path);
    let lifecycle = build_lifecycle(&opts.vm, opts.no_suspend, &opts.hypervisor);
    let store = opts.store.as_deref().unwrap_or(&cfg.store);
    let run_opts = RunBackupOptions {
        path: &resolved_path,
        store,
        vm: &opts.vm,
        disk_name: &opts.disk_name,
        disk_format,
        cfg,
        encrypt: opts.encrypt.as_deref(),
        retain: opts.retain,
    };
    run_backup_with_lock(&run_opts, lifecycle)
}
