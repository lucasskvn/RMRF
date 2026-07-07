use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(Deserialize, Debug, Clone)]
pub struct Job {
    pub name: String,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub vm: Option<String>,
    pub schedule: String,
    #[serde(default)]
    pub retain: Option<u64>,
    #[serde(default)]
    pub encrypt: Option<String>,
    #[serde(default)]
    pub live: bool,
    #[serde(default)]
    pub disk: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct RmrfConfig {
    #[serde(default = "default_store")]
    pub store: PathBuf,
    #[serde(default = "default_compression_level")]
    pub compression_level: i32,
    #[serde(default = "default_cdc_min")]
    pub cdc_min: u32,
    #[serde(default = "default_cdc_avg")]
    pub cdc_avg: u32,
    #[serde(default = "default_cdc_max")]
    pub cdc_max: u32,
    #[serde(default)]
    pub jobs: Vec<Job>,
}

fn default_store() -> PathBuf {
    std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".rmrf-store"))
        .unwrap_or_else(|| PathBuf::from("./rmrf-store"))
}

fn default_compression_level() -> i32 {
    10
}

fn default_cdc_min() -> u32 {
    4096
}

fn default_cdc_avg() -> u32 {
    16384
}

fn default_cdc_max() -> u32 {
    65536
}

impl Default for RmrfConfig {
    fn default() -> Self {
        RmrfConfig {
            store: default_store(),
            compression_level: default_compression_level(),
            cdc_min: default_cdc_min(),
            cdc_avg: default_cdc_avg(),
            cdc_max: default_cdc_max(),
            jobs: Vec::new(),
        }
    }
}

pub fn parse_cron_schedule(schedule: &str) -> Result<cron::Schedule, String> {
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    let normalized = if fields.len() == 5 {
        format!("0 {schedule}")
    } else {
        schedule.to_string()
    };
    cron::Schedule::from_str(&normalized)
        .map_err(|e| format!("invalid cron schedule {schedule:?}: {e}"))
}

pub fn validate_job_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("job name is empty".into());
    }
    if name.len() > 64 {
        return Err(format!("job name is too long (max 64): {name:?}"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(format!(
            "job name contains disallowed characters (allowed: A-Z a-z 0-9 . _ -): {name:?}"
        ));
    }
    Ok(())
}

impl RmrfConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=22).contains(&self.compression_level) {
            return Err(format!(
                "compression_level must be in 1..=22, got {}",
                self.compression_level
            ));
        }
        if self.cdc_min == 0 || self.cdc_min > self.cdc_avg || self.cdc_avg > self.cdc_max {
            return Err(format!(
                "cdc sizes must satisfy 0 < cdc_min <= cdc_avg <= cdc_max (got min={} avg={} max={})",
                self.cdc_min, self.cdc_avg, self.cdc_max
            ));
        }
        for job in &self.jobs {
            validate_job_name(&job.name)?;
            parse_cron_schedule(&job.schedule)?;
        }
        Ok(())
    }
}

pub fn load_config() -> RmrfConfig {
    let Some(home) = std::env::var_os("HOME") else {
        return RmrfConfig::default();
    };
    let path = PathBuf::from(home).join(".rmrf.toml");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return RmrfConfig::default();
    };
    match toml::from_str::<RmrfConfig>(&content) {
        Ok(cfg) => match cfg.validate() {
            Ok(()) => cfg,
            Err(e) => {
                eprintln!(
                    "Warning: invalid config at {}: {e}; using defaults",
                    path.display()
                );
                RmrfConfig::default()
            }
        },
        Err(e) => {
            eprintln!(
                "Warning: failed to parse config at {}: {e}; using defaults",
                path.display()
            );
            RmrfConfig::default()
        }
    }
}

pub fn load_config_file(path: &Path) -> Result<RmrfConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read config {}: {e}", path.display()))?;
    let cfg: RmrfConfig =
        toml::from_str(&content).map_err(|e| format!("invalid config {}: {e}", path.display()))?;
    cfg.validate()
        .map_err(|e| format!("invalid config {}: {e}", path.display()))?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = RmrfConfig::default();
        let expected_store = std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".rmrf-store"))
            .unwrap_or_else(|| PathBuf::from("./rmrf-store"));
        assert_eq!(cfg.store, expected_store);
        if std::env::var_os("HOME").is_some() {
            assert!(cfg.store.is_absolute(), "default store should be absolute");
        }
        assert_eq!(cfg.compression_level, 10);
        assert_eq!(cfg.cdc_min, 4096);
        assert_eq!(cfg.cdc_avg, 16384);
        assert_eq!(cfg.cdc_max, 65536);
    }

    #[test]
    fn config_validate_rejects_bad_compression_and_cdc() {
        fn cfg_with(compression: i32, min: u32, avg: u32, max: u32) -> RmrfConfig {
            RmrfConfig {
                compression_level: compression,
                cdc_min: min,
                cdc_avg: avg,
                cdc_max: max,
                ..Default::default()
            }
        }
        assert!(cfg_with(99, 4096, 16384, 65536).validate().is_err());
        assert!(
            cfg_with(10, 0, 16384, 65536).validate().is_err(),
            "cdc_min=0"
        );
        assert!(
            cfg_with(10, 4096, 1000, 65536).validate().is_err(),
            "avg < min"
        );
        assert!(
            cfg_with(10, 4096, 16384, 100).validate().is_err(),
            "max < avg"
        );
    }

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
    fn config_validate_rejects_bad_job_name_and_schedule() {
        let bad_name = RmrfConfig {
            jobs: vec![make_job("evil; rm -rf /", "0 3 * * *")],
            ..Default::default()
        };
        assert!(
            bad_name.validate().is_err(),
            "metacharacters in job name must be rejected"
        );
        let bad_sched = RmrfConfig {
            jobs: vec![make_job("good-job", "not a cron schedule")],
            ..Default::default()
        };
        assert!(
            bad_sched.validate().is_err(),
            "invalid cron schedule must be rejected"
        );
        let ok = RmrfConfig {
            jobs: vec![make_job("good-job", "0 3 * * *")],
            ..Default::default()
        };
        assert!(ok.validate().is_ok());
    }

    #[test]
    fn config_validate_accepts_defaults() {
        assert!(RmrfConfig::default().validate().is_ok());
    }

    #[test]
    fn parse_cron_schedule_accepts_five_and_six_fields() {
        assert!(parse_cron_schedule("0 3 * * *").is_ok());
        assert!(parse_cron_schedule("0 0 3 * * *").is_ok());
        assert!(parse_cron_schedule("@daily").is_ok());
        assert!(parse_cron_schedule("not a cron schedule").is_err());
        assert!(parse_cron_schedule("").is_err());
    }

    #[test]
    fn config_parse_toml() {
        let toml_str = r#"
            store = "/tmp/my-store"
            compression_level = 3
            cdc_min = 2048
            cdc_avg = 8192
            cdc_max = 32768
        "#;
        let cfg: RmrfConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.store, PathBuf::from("/tmp/my-store"));
        assert_eq!(cfg.compression_level, 3);
        assert_eq!(cfg.cdc_min, 2048);
        assert_eq!(cfg.cdc_avg, 8192);
        assert_eq!(cfg.cdc_max, 32768);
    }

    #[test]
    fn config_partial_toml_uses_defaults() {
        let toml_str = r#"store = "/custom""#;
        let cfg: RmrfConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.store, PathBuf::from("/custom"));
        assert_eq!(cfg.compression_level, 10);
        assert_eq!(cfg.cdc_min, 4096);
    }
}
