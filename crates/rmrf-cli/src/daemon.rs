use crate::config::{Job, RmrfConfig};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

static DAEMON_LOCK: Mutex<()> = Mutex::new(());

fn lock_path() -> PathBuf {
    PathBuf::from("/tmp/rmrf-daemon.lock")
}

fn log_path() -> std::path::PathBuf {
    std::env::var_os("HOME")
        .map(|h| Path::new(&h).join(".rmrf-daemon.log"))
        .unwrap_or_else(|| Path::new("/tmp/rmrf-daemon.log").to_path_buf())
}

fn log_msg(msg: &str) {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!("[{timestamp}] {msg}\n");
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path())
    {
        let _ = f.write_all(line.as_bytes());
    }
    eprint!("{line}");
}

/// Returns true if the PID in the lock file is a live process.
fn pid_alive(pid: u32) -> bool {
    // `kill -0 <pid>` exits 0 iff the process exists and is signalable.
    // We must check `.success()`, not `.is_ok()` — the latter only reports
    // whether `kill` could be spawned, so a dead PID would read as "alive".
    process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn write_lock(lock_path: &Path) -> bool {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(lock_path)
    {
        Ok(f) => f,
        Err(_) => return false,
    };
    file.write_all(process::id().to_string().as_bytes()).ok();
    file.sync_all().ok();
    true
}

fn try_lock_daemon() -> bool {
    let lock_path = lock_path();
    if write_lock(&lock_path) {
        return true;
    }
    // Lock exists — only legitimate if its holder is alive.
    if let Ok(pid_str) = fs::read_to_string(&lock_path) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if pid_alive(pid) {
                log_msg("ERROR: another daemon instance is running");
                return false;
            }
            log_msg(&format!(
                "Warning: removing stale daemon lock from PID {pid}"
            ));
        }
    }
    let _ = fs::remove_file(&lock_path);
    write_lock(&lock_path)
}

/// RAII guard that releases the daemon lock on drop, so an early return or
/// panic cannot orphan the lock.
struct DaemonGuard {
    lock_path: PathBuf,
    active: bool,
}

impl DaemonGuard {
    fn new() -> Option<Self> {
        if try_lock_daemon() {
            Some(DaemonGuard {
                lock_path: lock_path(),
                active: true,
            })
        } else {
            None
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = fs::remove_file(&self.lock_path);
            self.active = false;
        }
    }
}

fn run_job(job: &Job, cfg: &RmrfConfig) -> bool {
    let store = &cfg.store;
    let _lock = DAEMON_LOCK.lock().unwrap();

    log_msg(&format!("JOB {}: starting", job.name));

    let start = chrono::Utc::now();

    let mut cmd = process::Command::new(std::env::current_exe().unwrap_or_else(|_| "rmrf".into()));
    cmd.arg("backup");
    cmd.arg("-s");
    cmd.arg(store);

    if let Some(path) = &job.path {
        cmd.arg(path);
    }
    if let Some(vm) = &job.vm {
        cmd.arg("--vm");
        cmd.arg(vm);
    }
    if let Some(disk) = &job.disk {
        cmd.arg("--disk");
        cmd.arg(disk);
    }
    if let Some(retain) = job.retain {
        cmd.arg("--retain");
        cmd.arg(retain.to_string());
    }
    if let Some(encrypt) = &job.encrypt {
        cmd.arg("--encrypt");
        cmd.arg(encrypt);
    }
    if job.live {
        cmd.arg("--live");
    }

    let output = cmd.output();
    match output {
        Ok(out) => {
            let elapsed = (chrono::Utc::now() - start).num_seconds();
            if out.status.success() {
                log_msg(&format!("JOB {}: success ({}s)", job.name, elapsed));
                true
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                log_msg(&format!(
                    "JOB {}: FAILED ({}s): {}",
                    job.name,
                    elapsed,
                    stderr.lines().next().unwrap_or("unknown error")
                ));
                false
            }
        }
        Err(e) => {
            log_msg(&format!("JOB {}: FAILED to launch: {e}", job.name));
            false
        }
    }
}

pub fn run_daemon(once: bool, job_name: Option<&str>, cfg: &RmrfConfig) -> bool {
    let _guard = match DaemonGuard::new() {
        Some(g) => g,
        None => return false,
    };

    let jobs: Vec<&Job> = if let Some(name) = job_name {
        cfg.jobs.iter().filter(|j| j.name == name).collect()
    } else {
        cfg.jobs.iter().collect()
    };

    if jobs.is_empty() {
        log_msg("ERROR: no matching jobs found in config");
        return false;
    }

    if once {
        for job in &jobs {
            run_job(job, cfg);
        }
        return true;
    }

    log_msg(&format!("Daemon started with {} job(s)", jobs.len()));

    loop {
        let now = chrono::Utc::now();
        let mut next_run: Option<(chrono::DateTime<chrono::Utc>, &Job)> = None;

        for job in &jobs {
            if let Ok(schedule) = cron::Schedule::from_str(&job.schedule) {
                if let Some(next) = schedule.upcoming(chrono::Utc).next() {
                    match next_run {
                        Some((t, _)) if next < t => next_run = Some((next, job)),
                        None => next_run = Some((next, job)),
                        _ => {}
                    }
                }
            }
        }

        if let Some((next, _)) = next_run {
            let sleep_secs = (next - now).num_seconds().max(1) as u64;
            log_msg(&format!(
                "Next job at {}, sleeping {}s",
                next.format("%Y-%m-%dT%H:%M:%SZ"),
                sleep_secs
            ));
            std::thread::sleep(Duration::from_secs(sleep_secs.min(3600)));

            for job in &jobs {
                if let Ok(schedule) = cron::Schedule::from_str(&job.schedule) {
                    if let Some(next) = schedule.upcoming(chrono::Utc).next() {
                        if (next - chrono::Utc::now()).num_seconds().abs() < 5 {
                            run_job(job, cfg);
                        }
                    }
                }
            }
        } else {
            log_msg("No upcoming jobs, sleeping 1h");
            std::thread::sleep(Duration::from_secs(3600));
        }
    }
}
