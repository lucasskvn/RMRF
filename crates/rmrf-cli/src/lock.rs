use std::path::Path;

const STORE_LOCK: &str = ".rmrf.lock";

pub fn try_lock_store(store: &Path) -> bool {
    if let Err(e) = std::fs::create_dir_all(store) {
        eprintln!("Error: cannot create store directory: {e}");
        return false;
    }
    let lock_path = store.join(STORE_LOCK);
    if let Ok(pid_str) = std::fs::read_to_string(&lock_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let alive = Path::new(&format!("/proc/{pid}")).exists();
            if alive {
                eprintln!("Error: store is locked by process {pid}");
                return false;
            }
            eprintln!("Warning: removing stale lock from PID {pid}");
        }
    }
    if let Err(e) = std::fs::write(&lock_path, format!("{}", std::process::id())) {
        eprintln!("Error: cannot write store lock: {e}");
        return false;
    }
    true
}

pub fn unlock_store(store: &Path) {
    let lock_path = store.join(STORE_LOCK);
    std::fs::remove_file(&lock_path).ok();
}

pub fn store_sentinel_path(store: &Path, vm_id: &str) -> std::path::PathBuf {
    store.join(format!(".suspended-{vm_id}"))
}

pub fn check_stale_sentinels(store: &Path) {
    if !store.exists() {
        return;
    }
    if let Ok(dir) = std::fs::read_dir(store) {
        for entry in dir.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(vm_id) = name_str.strip_prefix(".suspended-") {
                eprintln!(
                    "Warning: VM {vm_id} may be suspended — sentinel file {} exists",
                    entry.path().display()
                );
                eprintln!("  This can happen if the backup process was killed");
                eprintln!("  Check VM status: qm status {vm_id} | pct status {vm_id}");
            }
        }
    }
}
