use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn rmrf() -> Command {
    Command::cargo_bin("rmrf").unwrap()
}

#[test]
fn backup_then_restore_bit_identical() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("data.bin");
    std::fs::write(&src, b"hello world this is a test file for rmrf").unwrap();

    rmrf().args(["backup", src.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success();

    let manifest = std::fs::read_dir(store.path().join("manifests")).unwrap()
        .next().unwrap().unwrap().path();
    let out = work.path().join("restored");
    rmrf().args(["restore", manifest.to_str().unwrap(),
        "-s", store.path().to_str().unwrap(),
        "-o", out.to_str().unwrap()])
        .assert().success();

    assert_eq!(std::fs::read(&src).unwrap(), std::fs::read(&out).unwrap());
}

#[test]
fn dedup_second_backup_no_new_chunks() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("data.bin");
    std::fs::write(&src, b"dedup test data for rmrf").unwrap();

    rmrf().args(["backup", src.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success();

    let output = rmrf().args(["backup", src.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);
    assert!(stdout.contains("Dedup skipped") || stdout.contains("dedup"));
}

#[test]
fn list_shows_backup() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("list_test.bin");
    std::fs::write(&src, b"list test").unwrap();

    rmrf().args(["backup", src.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success();

    rmrf().args(["list", "-s", store.path().to_str().unwrap()])
        .assert().success().stdout(predicate::str::contains("list_test.bin"));
}

#[test]
fn verify_passes_after_backup() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("verify_test.bin");
    std::fs::write(&src, b"verify test data for rmrf").unwrap();

    rmrf().args(["backup", src.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success();

    let manifest = std::fs::read_dir(store.path().join("manifests")).unwrap()
        .next().unwrap().unwrap().path();
    rmrf().args(["verify", manifest.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success();
}

#[test]
fn restore_force_overwrites() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("force_test.bin");
    std::fs::write(&src, b"force test").unwrap();

    rmrf().args(["backup", src.to_str().unwrap(), "-s", store.path().to_str().unwrap()])
        .assert().success();

    let manifest = std::fs::read_dir(store.path().join("manifests")).unwrap()
        .next().unwrap().unwrap().path();
    let out = work.path().join("restored");
    std::fs::write(&out, b"existing content").unwrap();

    rmrf().args(["restore", manifest.to_str().unwrap(),
        "-s", store.path().to_str().unwrap(),
        "-o", out.to_str().unwrap()])
        .assert().failure();

    rmrf().args(["restore", manifest.to_str().unwrap(),
        "-s", store.path().to_str().unwrap(),
        "-o", out.to_str().unwrap(), "--force"])
        .assert().success();
}
