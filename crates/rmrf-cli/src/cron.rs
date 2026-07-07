use crate::config::{validate_job_name, Job};
use std::path::Path;

pub fn generate_crontab(jobs: &[Job], binary: &Path) -> String {
    if jobs.is_empty() {
        return String::new();
    }
    let mut out = String::from("# RMRF scheduled jobs\n");
    for job in jobs {
        if let Err(e) = validate_job_name(&job.name) {
            out.push_str(&format!("# WARNING: skipping job with unsafe name: {e}\n"));
            continue;
        }
        out.push_str(&format!(
            "# RMRF: {name}\n{schedule} {binary} daemon --once --job {name}\n",
            name = job.name,
            schedule = job.schedule,
            binary = binary.display(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job(name: &str, schedule: &str) -> Job {
        Job {
            name: name.to_string(),
            path: None,
            vm: None,
            schedule: schedule.to_string(),
            retain: None,
            encrypt: None,
            live: false,
            disk: None,
        }
    }

    #[test]
    fn cron_empty_jobs() {
        assert_eq!(generate_crontab(&[], Path::new("/usr/bin/rmrf")), "");
    }

    #[test]
    fn cron_single_job() {
        let jobs = vec![make_job("test", "0 3 * * *")];
        let out = generate_crontab(&jobs, Path::new("/usr/local/bin/rmrf"));
        assert!(out.contains("0 3 * * *"));
        assert!(out.contains("/usr/local/bin/rmrf"));
        assert!(out.contains("test"));
    }

    #[test]
    fn cron_multiple_jobs() {
        let jobs = vec![
            make_job("daily", "0 3 * * *"),
            make_job("hourly", "0 * * * *"),
        ];
        let out = generate_crontab(&jobs, Path::new("/usr/bin/rmrf"));
        assert!(out.contains("daily"));
        assert!(out.contains("hourly"));
        assert_eq!(out.matches("rmrf daemon --once").count(), 2);
    }
}
