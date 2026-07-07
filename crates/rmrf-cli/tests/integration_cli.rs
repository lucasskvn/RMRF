use assert_cmd::Command;
use predicates::prelude::*;

fn rmrf() -> Command {
    Command::cargo_bin("rmrf").unwrap()
}

#[test]
fn no_args_prints_help() {
    rmrf()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage"));
}

#[test]
fn backup_missing_file_exits_nonzero() {
    let store = tempfile::TempDir::new().unwrap();
    rmrf()
        .args([
            "backup",
            "/nonexistent/path/file.bin",
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn restore_nonexistent_manifest_exits_nonzero() {
    let store = tempfile::TempDir::new().unwrap();
    rmrf()
        .args([
            "restore",
            "/nonexistent/manifest.bincode",
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn gc_dry_run_on_empty_store_succeeds() {
    let store = tempfile::TempDir::new().unwrap();
    rmrf()
        .args(["gc", "-s", store.path().to_str().unwrap(), "--dry-run"])
        .assert()
        .success();
}

#[test]
fn info_on_nonexistent_manifest_fails() {
    let store = tempfile::TempDir::new().unwrap();
    rmrf()
        .args([
            "info",
            "/nonexistent/manifest.bincode",
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .failure();
}
