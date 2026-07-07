use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn rmrf() -> Command {
    Command::cargo_bin("rmrf").unwrap()
}

#[test]
fn live_backup_is_disabled() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("disk.img");
    fs::write(&src, b"x").unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--live",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("disabled"));
}

#[test]
fn backup_rejects_invalid_vm_id_path_traversal() {
    let store = TempDir::new().unwrap();
    rmrf()
        .args([
            "backup",
            "-s",
            store.path().to_str().unwrap(),
            "--vm",
            "../etc/passwd",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("VM id"));
}

#[test]
fn backup_rejects_invalid_vm_id_leading_dash() {
    let store = TempDir::new().unwrap();
    rmrf()
        .args(["backup", "-s", store.path().to_str().unwrap(), "--vm=-100"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("VM id"));
}
