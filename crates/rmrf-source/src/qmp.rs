use rmrf_core::{Result, RmrfError};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

const QMP_READ_TIMEOUT: Duration = Duration::from_secs(30);

pub struct QmpClient {
    stream: BufReader<UnixStream>,
}

impl QmpClient {

    pub fn connect(socket_path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).map_err(|e| {
            RmrfError::Msg(format!("QMP connect to {}: {e}", socket_path.display()))
        })?;
        stream
            .set_read_timeout(Some(QMP_READ_TIMEOUT))
            .map_err(|e| RmrfError::Msg(format!("QMP set read timeout: {e}")))?;
        let mut reader = BufReader::new(stream);
        let mut greeting = String::new();
        reader
            .read_line(&mut greeting)
            .map_err(|e| RmrfError::Msg(format!("QMP read greeting: {e}")))?;
        let val: serde_json::Value = serde_json::from_str(&greeting)
            .map_err(|e| RmrfError::Msg(format!("QMP parse greeting: {e}")))?;
        if val.get("QMP").is_none() {
            return Err(RmrfError::Msg("QMP greeting missing QMP key".into()));
        }
        let stream = reader.into_inner();
        let mut client = QmpClient {
            stream: BufReader::new(stream),
        };
        client.execute("qmp_capabilities", serde_json::json!({}))?;
        Ok(client)
    }

    fn send_request(&mut self, cmd: &str, args: serde_json::Value) -> Result<()> {
        let request = serde_json::json!({
            "execute": cmd,
            "arguments": args,
        });
        let mut req_bytes = serde_json::to_vec(&request)
            .map_err(|e| RmrfError::Msg(format!("QMP serialize: {e}")))?;
        req_bytes.push(b'\n');
        self.stream
            .get_mut()
            .write_all(&req_bytes)
            .map_err(|e| RmrfError::Msg(format!("QMP write: {e}")))?;
        self.stream
            .get_mut()
            .flush()
            .map_err(|e| RmrfError::Msg(format!("QMP flush: {e}")))?;
        Ok(())
    }

    fn read_response(&mut self) -> Result<serde_json::Value> {
        loop {
            let mut line = String::new();
            self.stream
                .read_line(&mut line)
                .map_err(|e| RmrfError::Msg(format!("QMP read response: {e}")))?;
            if line.trim().is_empty() {
                continue;
            }
            let val: serde_json::Value = serde_json::from_str(&line)
                .map_err(|e| RmrfError::Msg(format!("QMP parse response: {e}")))?;
            if val.get("event").is_some() {
                continue;
            }
            if val.get("error").is_some() {
                return Err(RmrfError::Msg(format!("QMP error: {val}")));
            }
            return Ok(val);
        }
    }

    fn execute(&mut self, cmd: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        self.send_request(cmd, args)?;
        self.read_response()
    }

    pub fn create_snapshot(&mut self, device: &str, snapshot_file: &Path, format: &str) -> Result<()> {
        let args = serde_json::json!({
            "device": device,
            "snapshot-file": snapshot_file.to_string_lossy(),
            "format": format,
        });
        self.execute("blockdev-snapshot-sync", args)?;
        Ok(())
    }

    pub fn close(self) {}
}
