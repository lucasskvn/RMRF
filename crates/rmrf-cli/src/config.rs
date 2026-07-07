use serde::Deserialize;
use std::path::PathBuf;

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
}

fn default_store() -> PathBuf {
    PathBuf::from("./rmrf-store")
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
        }
    }
}

pub fn load_config() -> RmrfConfig {
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home).join(".rmrf.toml");
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = toml::from_str(&content) {
                return cfg;
            }
        }
    }
    RmrfConfig::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_values() {
        let cfg = RmrfConfig::default();
        assert_eq!(cfg.store, PathBuf::from("./rmrf-store"));
        assert_eq!(cfg.compression_level, 10);
        assert_eq!(cfg.cdc_min, 4096);
        assert_eq!(cfg.cdc_avg, 16384);
        assert_eq!(cfg.cdc_max, 65536);
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
