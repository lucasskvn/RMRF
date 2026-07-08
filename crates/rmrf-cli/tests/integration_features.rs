use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn rmrf() -> Command {
    Command::cargo_bin("rmrf").unwrap()
}

fn write_config(dir: &std::path::Path, content: &str) -> std::path::PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, content).unwrap();
    path
}

#[test]
fn backup_with_config_file() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let cfg_dir = TempDir::new().unwrap();
    let src = work.path().join("data.bin");
    std::fs::write(&src, b"config test data").unwrap();

    let cfg = write_config(
        cfg_dir.path(),
        &format!(
            "store = \"{}\"\ncompression_level = 3\n",
            store.path().display()
        ),
    );

    rmrf()
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "backup",
            src.to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest = std::fs::read_dir(store.path().join("manifests"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let out = work.path().join("restored");
    rmrf()
        .args([
            "--config",
            cfg.to_str().unwrap(),
            "restore",
            manifest.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&out).unwrap());
}

#[test]
fn verify_rehash_passes() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("rehash_test.bin");
    std::fs::write(&src, b"data for rehash verification").unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest = std::fs::read_dir(store.path().join("manifests"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    rmrf()
        .args([
            "verify",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--rehash",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("re-hashed"));
}

#[test]
fn list_with_since_filter() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("filter_test.bin");
    std::fs::write(&src, b"filter test").unwrap();

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
            "20200101",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("filter_test.bin"));

    rmrf()
        .args([
            "list",
            "-s",
            store.path().to_str().unwrap(),
            "--before",
            "20200101",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn list_with_source_filter() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("unique_name_xyz.bin");
    std::fs::write(&src, b"source filter test").unwrap();

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
            "--source",
            "unique_name",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("unique_name_xyz.bin"));
}

#[test]
fn parallel_restore_with_jobs() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("parallel_test.bin");
    let data = vec![0xABu8; 100_000];
    std::fs::write(&src, &data).unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest = std::fs::read_dir(store.path().join("manifests"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let out = work.path().join("restored_parallel");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--jobs",
            "4",
        ])
        .assert()
        .success();

    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&out).unwrap());
}

#[test]
fn gc_dry_run_after_backup() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("gc_test.bin");
    std::fs::write(&src, b"gc test data").unwrap();

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
        .args(["gc", "-s", store.path().to_str().unwrap(), "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("would delete 0 orphan chunks"));
}

#[test]
fn retention_dry_run_lists_without_deleting() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("retain_dry.bin");
    fs::write(&src, b"retention dry-run test").unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest_count_before = fs::read_dir(store.path().join("manifests"))
        .unwrap()
        .count();

    rmrf()
        .args([
            "retention",
            "-s",
            store.path().to_str().unwrap(),
            "--days",
            "36500",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run: no manifests"));

    let manifest_count_after = fs::read_dir(store.path().join("manifests"))
        .unwrap()
        .count();
    assert_eq!(
        manifest_count_before, manifest_count_after,
        "dry-run must not delete manifests"
    );
}
