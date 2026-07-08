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
