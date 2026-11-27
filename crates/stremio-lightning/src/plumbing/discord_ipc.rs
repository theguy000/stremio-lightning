//! Minimal Discord Rich Presence IPC v1 transport over Windows named pipes or Unix domain sockets.

use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

const OP_HANDSHAKE: u32 = 0;
const OP_FRAME: u32 = 1;
const OP_CLOSE: u32 = 2;

pub(super) struct IpcConnection {
    #[cfg(target_os = "windows")]
    pipe: std::fs::File,
    #[cfg(unix)]
    stream: std::os::unix::net::UnixStream,
}

impl IpcConnection {
    pub(super) fn connect(app_id: &str) -> Result<Self, String> {
        let mut conn = open_transport()?;
        let handshake = json!({ "v": 1, "client_id": app_id });
        conn.write_frame(OP_HANDSHAKE, &handshake)?;
        let (op, _) = conn.read_frame()?;
        if op != OP_FRAME {
            return Err(format!("unexpected Discord handshake opcode {op}"));
        }
        Ok(conn)
    }

    pub(super) fn set_activity(&mut self, activity: Option<&Value>) -> Result<(), String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos().to_string())
            .unwrap_or_else(|_| "0".to_owned());
        let args = match activity {
            Some(act) => json!({ "pid": std::process::id(), "activity": act }),
            None => json!({ "pid": std::process::id() }),
        };
        let payload = json!({
            "cmd": "SET_ACTIVITY",
            "args": args,
            "nonce": nonce,
        });
        self.write_frame(OP_FRAME, &payload)?;
        let (op, _) = self.read_frame()?;
        if op == OP_CLOSE {
            return Err("Discord closed IPC connection".to_owned());
        }
        Ok(())
    }

    pub(super) fn close(mut self) {
        let _ = self.write_frame(OP_CLOSE, &json!({}));
    }

    fn write_frame(&mut self, opcode: u32, body: &Value) -> Result<(), String> {
        let bytes = serde_json::to_vec(body).map_err(|e| e.to_string())?;
        let mut buf = Vec::with_capacity(8 + bytes.len());
        buf.extend_from_slice(&opcode.to_le_bytes());
        buf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        buf.extend_from_slice(&bytes);
        self.io_write(&buf).map_err(|e| e.to_string())
    }

    fn read_frame(&mut self) -> Result<(u32, Value), String> {
        let mut header = [0u8; 8];
        self.io_read(&mut header).map_err(|e| e.to_string())?;
        let op = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
        if len > 1024 * 1024 {
            return Err("Discord IPC frame too large".to_owned());
        }
        let mut body = vec![0u8; len];
        self.io_read(&mut body).map_err(|e| e.to_string())?;
        let val = serde_json::from_slice(&body).unwrap_or(Value::Null);
        Ok((op, val))
    }

    #[cfg(target_os = "windows")]
    fn io_write(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.pipe.write_all(buf)
    }

    #[cfg(target_os = "windows")]
    fn io_read(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.pipe.read_exact(buf)
    }

    #[cfg(unix)]
    fn io_write(&mut self, buf: &[u8]) -> std::io::Result<()> {
        self.stream.write_all(buf)
    }

    #[cfg(unix)]
    fn io_read(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.stream.read_exact(buf)
    }
}

#[cfg(target_os = "windows")]
fn open_transport() -> Result<IpcConnection, String> {
    use std::fs::OpenOptions;
    for i in 0..10 {
        let path = format!(r"\\?\pipe\discord-ipc-{i}");
        if let Ok(pipe) = OpenOptions::new().read(true).write(true).open(&path) {
            return Ok(IpcConnection { pipe });
        }
    }
    Err("Discord IPC pipe not found".to_owned())
}

#[cfg(unix)]
fn open_transport() -> Result<IpcConnection, String> {
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::time::Duration;

    let mut dirs: Vec<PathBuf> = Vec::new();
    for var in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                let p = PathBuf::from(&v);
                dirs.push(p.clone());
                dirs.push(p.join("app/com.discordapp.Discord"));
                dirs.push(p.join("snap.discord"));
            }
        }
    }
    dirs.push(PathBuf::from("/tmp"));

    for dir in dirs {
        for i in 0..10 {
            let path = dir.join(format!("discord-ipc-{i}"));
            if let Ok(stream) = UnixStream::connect(&path) {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
                return Ok(IpcConnection { stream });
            }
        }
    }
    Err("Discord IPC socket not found".to_owned())
}
