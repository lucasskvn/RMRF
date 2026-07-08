use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

const STORE_LOCK: &str = ".rmrf.lock";

pub enum SentinelCheckResult {
    Clean,
    Recovered,
    Failed,
}

pub fn try_lock_store(store: &Path) -> bool {
    if let Err(e) = std::fs::create_dir_all(store) {
        eprintln!("Error: cannot create store directory: {e}");
        return false;
    }
    let lock_path = store.join(STORE_LOCK);

    if let Err(e) = try_create_lock(&lock_path) {
        match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                if !lock_is_stale(&lock_path) {
                    return false;
                }
                let _ = std::fs::remove_file(&lock_path);
                if let Err(e) = try_create_lock(&lock_path) {
                    eprintln!("Error: cannot acquire store lock: {e}");
                    return false;
                }
            }
            _ => {
                eprintln!("Error: cannot create store lock: {e}");
                return false;
            }
        }
    }
    true
}

fn try_create_lock(lock_path: &Path) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)?;
    file.write_all(format!("{}", std::process::id()).as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn lock_is_stale(lock_path: &Path) -> bool {
    let Ok(pid_str) = std::fs::read_to_string(lock_path) else {
        return true;
    };
    let Ok(pid) = pid_str.trim().parse::<u32>() else {
        eprintln!("Warning: removing unreadable store lock");
        return true;
    };
    let alive = Path::new(&format!("/proc/{pid}")).exists();
    if alive {
        eprintln!("Error: store is locked by process {pid}");
        false
    } else {
        eprintln!("Warning: removing stale lock from PID {pid}");
        true
    }
}

pub fn unlock_store(store: &Path) {
    let lock_path = store.join(STORE_LOCK);
    std::fs::remove_file(&lock_path).ok();
}

pub fn store_sentinel_path(store: &Path, vm_id: &str) -> std::path::PathBuf {
    store.join(format!(".suspended-{vm_id}"))
}

pub fn check_stale_sentinels(store: &Path) -> SentinelCheckResult {
    if !store.exists() {
        return SentinelCheckResult::Clean;
    }

    let lock_path = store.join(STORE_LOCK);
    if let Ok(pid_str) = std::fs::read_to_string(&lock_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if Path::new(&format!("/proc/{pid}")).exists() {
                return SentinelCheckResult::Clean;
            }
        }
    }

    let mut found_any = false;
    let mut any_failed = false;
    if let Ok(dir) = std::fs::read_dir(store) {
        for entry in dir.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(vm_id) = name_str.strip_prefix(".suspended-") {
                found_any = true;
                eprintln!(
                    "Warning: found stale sentinel for VM {vm_id} — attempting auto-resume"
                );
                if try_auto_resume(vm_id) {
                    eprintln!("  VM {vm_id} resumed successfully, removing sentinel");
                    std::fs::remove_file(entry.path()).ok();
                } else {
                    eprintln!("  Could not auto-resume VM {vm_id} — check manually: qm status {vm_id} | pct status {vm_id}");
                    any_failed = true;
                }
            }
        }
    }

    if !found_any {
        SentinelCheckResult::Clean
    } else if any_failed {
        SentinelCheckResult::Failed
    } else {
        SentinelCheckResult::Recovered
    }
}

fn try_auto_resume(vm_id: &str) -> bool {
    for (cmd, _) in &[("qm", "resume"), ("pct", "resume")] {
        if let Ok(output) = std::process::Command::new(cmd)
            .args(["status", vm_id])
            .output()
        {
            let status = String::from_utf8_lossy(&output.stdout);
            if status.contains("paused") || status.contains("suspended") {
                return std::process::Command::new(cmd)
                    .args(["resume", vm_id])
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
            }
        }
    }
    false
}

