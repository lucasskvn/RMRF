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

