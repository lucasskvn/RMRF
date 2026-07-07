use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn rmrf() -> Command {
    Command::cargo_bin("rmrf").unwrap()
}

fn manifest_names(store: &std::path::Path) -> Vec<String> {
    fs::read_dir(store.join("manifests"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect()
}

#[test]
fn directory_backup_includes_dotfiles() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let dir = work.path().join("home");
    fs::create_dir_all(dir.join(".config")).unwrap();
    fs::write(dir.join(".bashrc"), b"dotfile content").unwrap();
    fs::write(dir.join(".config/secret"), b"nested dotfile").unwrap();
    fs::write(dir.join("visible.txt"), b"plain").unwrap();

    rmrf()
        .args([
            "backup",
            dir.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    // Each file gets a manifest named `{timestamp}-{label}.bincode`. Dotfiles
    // must appear (previously they were silently skipped).
    let names = manifest_names(store.path());
    assert!(
        names.iter().any(|n| n.contains(".bashrc")),
        ".bashrc must be backed up, not skipped as a dotfile: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains(".config_secret")),
        "dotfile under .config/ must be backed up: {names:?}"
    );
}

#[test]
fn config_file_malformed_is_hard_error() {
    let work = TempDir::new().unwrap();
    let cfg = work.path().join("bad.toml");
    fs::write(&cfg, "this is = = not valid toml {{{").unwrap();

    rmrf()
        .args(["--config", cfg.to_str().unwrap(), "list"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("invalid config"));
}

#[test]
fn config_file_invalid_values_is_hard_error() {
    let work = TempDir::new().unwrap();
    let cfg = work.path().join("badvals.toml");
    fs::write(&cfg, "compression_level = 99\n").unwrap();

    rmrf()
        .args(["--config", cfg.to_str().unwrap(), "list"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("compression_level"));
}

#[test]
fn list_rejects_unparseable_since() {
    let store = TempDir::new().unwrap();
    // Back up one file so the store has a manifests dir; otherwise list would
    // fail on the missing directory before validating --since.
    let work = TempDir::new().unwrap();
    let src = work.path().join("f.txt");
    fs::write(&src, b"data").unwrap();
    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();
    rmrf()
        .args([
            "list",
            "-s",
            store.path().to_str().unwrap(),
            "--since",
            "not-a-date",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("--since"));
}

#[test]
fn retention_preserves_malformed_name_manifests() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("r.bin");
    fs::write(&src, b"retention test data").unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifests_dir = store.path().join("manifests");
    let real: std::path::PathBuf = fs::read_dir(&manifests_dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .next()
        .unwrap();

    // An old, correctly-named manifest (year 2000) — eligible for deletion.
    let old_name = manifests_dir.join("20000101T000000Z-old.bincode");
    fs::copy(&real, &old_name).unwrap();
    // A malformed-name manifest (no timestamp) — must be skipped, not deleted.
    let malformed = manifests_dir.join("manual-backup.bincode");
    fs::copy(&real, &malformed).unwrap();

    // --retain 1 day: the 2000-era manifest is older than the cutoff and must
    // be removed; the malformed-name one must be preserved.
    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--retain",
            "1",
        ])
        .assert()
        .success();

    assert!(
        !old_name.exists(),
        "old correctly-named manifest must be deleted by retention"
    );
    assert!(
        malformed.exists(),
        "malformed-name manifest must be preserved, not treated as epoch 0 and deleted"
    );
}
