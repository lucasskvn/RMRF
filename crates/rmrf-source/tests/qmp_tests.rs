use rmrf_source::QmpClient;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn mock_qmp_server(socket_path: &Path, responses: Vec<&'static str>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let path = socket_path.to_path_buf();
    thread::spawn(move || {
        if path.exists() {
            std::fs::remove_file(&path).ok();
        }
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(false).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream.write_all(responses[0].as_bytes()).unwrap();
        stream.flush().unwrap();
        let mut buf = [0u8; 4096];
        for resp in &responses[1..] {
            let n = stream.read(&mut buf).unwrap();
            tx.send(String::from_utf8_lossy(&buf[..n]).to_string()).ok();
            stream.write_all(resp.as_bytes()).unwrap();
            stream.flush().unwrap();
        }
    });
    rx
}

fn wait_for_socket(path: &Path) {
    for _ in 0..50 {
        if path.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn qmp_handshake_success() {
    let dir = tempfile::TempDir::new().unwrap();
    let socket = dir.path().join("qmp.sock");
    let rx = mock_qmp_server(&socket, vec![
        "{\"QMP\":{\"version\":{\"qemu\":{\"major\":8,\"minor\":2,\"micro\":0}}}}\n",
        "{\"return\":{}}\n",
    ]);
    wait_for_socket(&socket);
    let client = QmpClient::connect(&socket).unwrap();
    let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(received.contains("qmp_capabilities"));
    client.close();
}

#[test]
fn qmp_handshake_rejects_bad_greeting() {
    let dir = tempfile::TempDir::new().unwrap();
    let socket = dir.path().join("bad.sock");
    let _rx = mock_qmp_server(&socket, vec![
        "{\"error\":{\"desc\":\"bad\"}}\n",
    ]);
    wait_for_socket(&socket);
    let result = QmpClient::connect(&socket);
    assert!(result.is_err());
}

#[test]
fn qmp_create_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let socket = dir.path().join("snap.sock");
    let rx = mock_qmp_server(&socket, vec![
        "{\"QMP\":{\"version\":{\"qemu\":{\"major\":8,\"minor\":2,\"micro\":0}}}}\n",
        "{\"return\":{}}\n",
        "{\"return\":{}}\n",
    ]);
    wait_for_socket(&socket);
    let mut client = QmpClient::connect(&socket).unwrap();
    let _ = rx.recv_timeout(Duration::from_secs(2));
    let snap = dir.path().join("snap.qcow2");
    client.create_snapshot("drive-virtio-disk0", &snap, "qcow2").unwrap();
    let received = rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(received.contains("blockdev-snapshot-sync"));
    assert!(received.contains("drive-virtio-disk0"));
    assert!(received.contains("snap.qcow2"));
    client.close();
}
