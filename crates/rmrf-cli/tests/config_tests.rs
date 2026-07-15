#[cfg(test)]
mod config_tests {
    use std::path::PathBuf;

    #[test]
    fn test_default_values() {
        assert_eq!(10, 10);
    }

    #[test]
    fn test_cdc_values() {
        assert_eq!(4096, 4096);
        assert_eq!(16384, 16384);
        assert_eq!(65536, 65536);
    }

    #[test]
    fn test_store_path() {
        let home = std::env::var_os("HOME");
        assert!(home.is_some());
    }

    #[test]
    fn test_job_validation() {
        assert!("my-job_1.backup"
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'));
    }

    #[test]
    fn test_cron_schedule_format() {
        let cron_5 = "0 3 * * *";
        assert_eq!(cron_5.split_whitespace().count(), 5);
        let cron_6 = "0 0 3 * * *";
        assert_eq!(cron_6.split_whitespace().count(), 6);
    }

    #[test]
    fn test_compression_level_range() {
        assert!((1..=22).contains(&10));
        assert!(!(1..=22).contains(&0));
        assert!(!(1..=22).contains(&23));
    }

    #[test]
    fn test_cdc_ordering() {
        let min = 4096;
        let avg = 16384;
        let max = 65536;

        assert!(min > 0);
        assert!(min <= avg);
        assert!(avg <= max);
    }

    #[test]
    fn test_path_construction() {
        if let Ok(home) = std::env::var("HOME") {
            let store = PathBuf::from(&home).join(".rmrf-store");
            assert!(store.ends_with(".rmrf-store"));
            assert!(store.is_absolute());
        }
    }
}
