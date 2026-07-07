use crate::config::{Job, RmrfConfig};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

static DAEMON_LOCK: Mutex<()> = Mutex::new(());

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

fn try_lock_daemon() -> bool {
    let lock_path = Path::new("/tmp/rmrf-daemon.lock");
    if lock_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(lock_path) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if process::Command::new("kill")
                    .args(["-0", &pid.to_string()])
                    .status()
                    .is_ok() {
                    log_msg("ERROR: another daemon instance is running");
                    return false;
                }
            }
        }
    }
    let pid = process::id();
    fs::write(lock_path, pid.to_string()).ok();
    true
}

fn unlock_daemon() {
    let _ = fs::remove_file(Path::new("/tmp/rmrf-daemon.lock"));
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
    if !try_lock_daemon() {
        return false;
    }

    let jobs: Vec<&Job> = if let Some(name) = job_name {
        cfg.jobs.iter().filter(|j| j.name == name).collect()
    } else {
        cfg.jobs.iter().collect()
    };

    if jobs.is_empty() {
        log_msg("ERROR: no matching jobs found in config");
        unlock_daemon();
        return false;
    }

    if once {
        for job in &jobs {
            run_job(job, cfg);
        }
        unlock_daemon();
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
