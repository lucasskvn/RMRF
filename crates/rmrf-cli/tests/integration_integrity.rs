use age::secrecy::ExposeSecret;
use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

fn rmrf() -> Command {
    Command::cargo_bin("rmrf").unwrap()
}

fn one_manifest(store: &std::path::Path) -> std::path::PathBuf {
    fs::read_dir(store.join("manifests"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .next()
        .unwrap()
}

fn first_chunk(store: &std::path::Path) -> std::path::PathBuf {
    for prefix in fs::read_dir(store.join("chunks")).unwrap().flatten() {
        if let Ok(inner) = fs::read_dir(prefix.path()) {
            if let Some(f) = inner.flatten().next() {
                return f.path();
            }
        }
    }
    panic!("no chunk file found in store");
}

fn count_chunks(store: &std::path::Path) -> usize {
    let mut n = 0;
    for prefix in fs::read_dir(store.join("chunks")).unwrap().flatten() {
        if let Ok(inner) = fs::read_dir(prefix.path()) {
            n += inner.flatten().count();
        }
    }
    n
}

#[test]
fn restore_corrupted_chunk_fails_without_partial_output() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("corrupt.bin");
    fs::write(&src, vec![0xCDu8; 200_000]).unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let chunk = first_chunk(store.path());
    let mut data = fs::read(&chunk).unwrap();
    data[10] ^= 0xFF;
    fs::write(&chunk, data).unwrap();

    let manifest = one_manifest(store.path());
    let out = work.path().join("restored");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .failure();

    assert!(!out.exists(), "no partial restore file should be left");
    assert!(
        !work.path().join("restored.tmp").exists(),
        "no tmp leftover"
    );
}

#[test]
fn gc_aborts_on_unreadable_manifest_keeps_chunks() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("gc_corrupt.bin");
    fs::write(&src, b"data to protect from gc").unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let chunks_before = count_chunks(store.path());
    let manifest = one_manifest(store.path());
    fs::write(&manifest, b"not a valid bincode manifest").unwrap();

    rmrf()
        .args(["gc", "-s", store.path().to_str().unwrap()])
        .assert()
        .failure();

    let chunks_after = count_chunks(store.path());
    assert_eq!(
        chunks_before, chunks_after,
        "GC must not delete chunks of an unreadable manifest"
    );
}

#[test]
fn manifest_save_leaves_no_tmp_file() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("atomic.bin");
    fs::write(&src, b"atomic save test").unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let tmp_files: Vec<_> = fs::read_dir(store.path().join("manifests"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(
        tmp_files.is_empty(),
        "no .tmp file should remain after atomic save"
    );
}

#[test]
fn restore_restores_modification_time() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("mtime.bin");
    fs::write(&src, b"mtime test data").unwrap();
    let fixed_mtime = 1_700_000_000;
    let _ = std::process::Command::new("touch")
        .args(["-d", &format!("@{fixed_mtime}"), src.to_str().unwrap()])
        .status();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest = one_manifest(store.path());
    let out = work.path().join("restored_mtime");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    let src_mtime = fs::metadata(&src).unwrap().modified().unwrap();
    let out_mtime = fs::metadata(&out).unwrap().modified().unwrap();
    assert_eq!(
        src_mtime, out_mtime,
        "restored file must keep original mtime"
    );
}

#[test]
fn restore_restores_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("perms.bin");
    fs::write(&src, b"permissions test data").unwrap();
    fs::set_permissions(&src, fs::Permissions::from_mode(0o600)).unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
        ])
        .assert()
        .success();

    let manifest = one_manifest(store.path());
    let out = work.path().join("restored_perms");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
        ])
        .assert()
        .success();

    let mode = fs::metadata(&out).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "restored file must keep original permissions");
}

#[test]
fn verify_encrypted_backup_with_identity() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("secret.bin");
    fs::write(&src, b"encrypted backup verify test").unwrap();

    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    let identity_file = work.path().join("age.key");
    fs::write(
        &identity_file,
        identity.to_string().expose_secret().as_bytes(),
    )
    .unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--encrypt",
            &recipient.to_string(),
        ])
        .assert()
        .success();

    let manifest = one_manifest(store.path());
    rmrf()
        .args([
            "verify",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--rehash",
            "--identity",
            identity_file.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("re-hashed"));

    let out = work.path().join("decrypted");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
            "--identity",
            identity_file.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(fs::read(&src).unwrap(), fs::read(&out).unwrap());
}

#[test]
fn encrypted_backup_with_raw_age_keygen_format() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("secret.bin");
    fs::write(&src, b"age-keygen raw format test").unwrap();

    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    let identity_file = work.path().join("age.key");
    let pubkey = recipient.to_string();
    let secret = identity.to_string().expose_secret().to_string();
    let raw_content = format!(
        "# created: 2026-07-08T13:14:12+02:00\n\
         # public key: {pubkey}\n\
         {secret}\n"
    );
    fs::write(&identity_file, raw_content).unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--encrypt",
            &pubkey,
        ])
        .assert()
        .success();

    let manifest = one_manifest(store.path());
    let out = work.path().join("decrypted");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
            "--identity",
            identity_file.to_str().unwrap(),
        ])
        .assert()
        .success();
    assert_eq!(fs::read(&src).unwrap(), fs::read(&out).unwrap());
}

#[test]
fn encrypted_backup_rejects_wrong_identity() {
    let store = TempDir::new().unwrap();
    let work = TempDir::new().unwrap();
    let src = work.path().join("secret.bin");
    fs::write(&src, b"wrong identity test").unwrap();

    let identity = age::x25519::Identity::generate();
    let recipient = identity.to_public();
    let identity_file = work.path().join("age.key");
    let pubkey = recipient.to_string();
    let secret = identity.to_string().expose_secret().to_string();
    let raw_content = format!(
        "# created: 2026-07-08T13:14:12+02:00\n\
         # public key: {pubkey}\n\
         {secret}\n"
    );
    fs::write(&identity_file, raw_content).unwrap();

    rmrf()
        .args([
            "backup",
            src.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "--encrypt",
            &pubkey,
        ])
        .assert()
        .success();

    let wrong_identity = age::x25519::Identity::generate();
    let wrong_file = work.path().join("wrong.key");
    let wrong_secret = wrong_identity.to_string().expose_secret().to_string();
    let wrong_pubkey = wrong_identity.to_public().to_string();
    let wrong_content = format!(
        "# created: 2026-07-08T13:14:12+02:00\n\
         # public key: {wrong_pubkey}\n\
         {wrong_secret}\n"
    );
    fs::write(&wrong_file, wrong_content).unwrap();

    let manifest = one_manifest(store.path());
    let out = work.path().join("decrypted");
    rmrf()
        .args([
            "restore",
            manifest.to_str().unwrap(),
            "-s",
            store.path().to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--force",
            "--identity",
            wrong_file.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("decryption failed"));
    assert!(!out.exists(), "no partial output on failed restore");
}
