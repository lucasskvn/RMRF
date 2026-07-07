use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

const STORE_LOCK: &str = ".rmrf.lock";

/// Try to acquire the store lock atomically.
///
/// Uses `O_CREAT | O_EXCL` so that lock creation is a single atomic step: two
/// processes racing for the lock cannot both succeed. A pre-existing lock is
/// only treated as stale (and reclaimed) when its PID is no longer alive,
/// which is checked *before* the reclaim to avoid resurrecting a lock another
/// process just legitimately acquired.
pub fn try_lock_store(store: &Path) -> bool {
    if let Err(e) = std::fs::create_dir_all(store) {
        eprintln!("Error: cannot create store directory: {e}");
        return false;
    }
    let lock_path = store.join(STORE_LOCK);

    if let Err(e) = try_create_lock(&lock_path) {
        // Lock exists — it may be stale (holder crashed). Check liveness and
        // reclaim only if the holder is dead.
        match e.kind() {
            std::io::ErrorKind::AlreadyExists => {
                if !lock_is_stale(&lock_path) {
                    return false;
                }
                // Stale: remove and retry once. Another racer may grab it first.
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

#[cfg(test)]
mod tests {
    use super::{try_lock_store, unlock_store};
    use tempfile::TempDir;

    #[test]
    fn first_lock_succeeds_second_blocks() {
        let store = TempDir::new().unwrap();
        assert!(
            try_lock_store(store.path()),
            "first acquisition must succeed"
        );
        assert!(
            !try_lock_store(store.path()),
            "second concurrent acquisition must be blocked"
        );
        unlock_store(store.path());
        assert!(
            try_lock_store(store.path()),
            "store must be lockable again after unlock"
        );
        unlock_store(store.path());
    }

    #[test]
    fn stale_lock_with_dead_pid_is_reclaimed() {
        let store = TempDir::new().unwrap();
        // Write a lock pointing at a PID that is extremely unlikely to exist,
        // simulating a crashed previous holder.
        let lock = store.path().join(".rmrf.lock");
        std::fs::write(&lock, "4194303").unwrap();
        assert!(
            try_lock_store(store.path()),
            "a stale lock (dead PID) must be reclaimed"
        );
        unlock_store(store.path());
    }

    #[test]
    fn lock_file_contains_current_pid() {
        let store = TempDir::new().unwrap();
        assert!(try_lock_store(store.path()));
        let contents = std::fs::read_to_string(store.path().join(".rmrf.lock")).unwrap();
        assert_eq!(contents.trim().parse::<u32>().unwrap(), std::process::id());
        unlock_store(store.path());
    }
}
