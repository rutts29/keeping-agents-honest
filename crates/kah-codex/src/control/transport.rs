use super::{ControlError, error};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};
use tungstenite::{Message, WebSocket, protocol::WebSocketConfig};

pub(super) enum Transport {
    Process {
        child: Child,
        stdin: ChildStdin,
        messages: Receiver<Result<Value, ControlError>>,
    },
    Daemon(Box<WebSocket<UnixStream>>),
}
impl Transport {
    pub(super) fn process(binary: &Path, home: &Path, cwd: &Path) -> Result<Self, ControlError> {
        let mut child = Command::new(binary)
            .args(["app-server", "--stdio"])
            .env("CODEX_HOME", home)
            .env_remove("HERDR_ENV")
            .env_remove("CODEX_THREAD_ID")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| error("CODEX_RPC_SPAWN", e))?;
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, messages) = mpsc::sync_channel(16);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let result = read_message(&mut reader);
                let failed = result.is_err();
                if tx.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self::Process {
            child,
            stdin,
            messages,
        })
    }
    pub(super) fn daemon(path: &Path, timeout: Duration) -> Result<Self, ControlError> {
        let stream = UnixStream::connect(path).map_err(|e| error("CODEX_RPC_SOCKET", e))?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .map_err(|e| error("CODEX_RPC_SOCKET", e))?;
        let config = WebSocketConfig::default()
            .max_message_size(None)
            .max_frame_size(None);
        let (socket, _) =
            tungstenite::client::client_with_config("ws://localhost/", stream, Some(config))
                .map_err(|e| error("CODEX_RPC_HANDSHAKE", e))?;
        Ok(Self::Daemon(Box::new(socket)))
    }
    pub(super) fn send(&mut self, value: &Value) -> Result<(), ControlError> {
        match self {
            Self::Process { stdin, .. } => {
                writeln!(stdin, "{value}").map_err(|e| error("CODEX_RPC_WRITE", e))
            }
            Self::Daemon(socket) => socket
                .send(Message::Text(value.to_string().into()))
                .map_err(ws_error),
        }
    }
    pub(super) fn receive(&mut self, deadline: Instant) -> Result<Value, ControlError> {
        match self {
            Self::Process { messages, .. } => messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| {
                    error(
                        if e == RecvTimeoutError::Timeout {
                            "CODEX_RPC_TIMEOUT"
                        } else {
                            "CODEX_RPC_CLOSED"
                        },
                        "native response",
                    )
                })?,
            Self::Daemon(socket) => receive_socket(socket, deadline),
        }
    }
}
fn receive_socket(
    socket: &mut WebSocket<UnixStream>,
    deadline: Instant,
) -> Result<Value, ControlError> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(error("CODEX_RPC_TIMEOUT", "native response"));
        }
        socket
            .get_mut()
            .set_read_timeout(Some(remaining))
            .map_err(|e| error("CODEX_RPC_SOCKET", e))?;
        match socket.read().map_err(ws_error)? {
            Message::Text(text) => {
                return serde_json::from_str(&text).map_err(|e| error("CODEX_RPC_JSON", e));
            }
            Message::Binary(bytes) => {
                return serde_json::from_slice(&bytes).map_err(|e| error("CODEX_RPC_JSON", e));
            }
            Message::Close(_) => return Err(error("CODEX_RPC_CLOSED", "daemon closed connection")),
            _ => socket.flush().map_err(ws_error)?,
        }
    }
}
fn read_message(reader: &mut impl BufRead) -> Result<Value, ControlError> {
    let mut bytes = Vec::new();
    let count = reader
        .read_until(b'\n', &mut bytes)
        .map_err(|e| error("CODEX_RPC_READ", e))?;
    if count == 0 {
        return Err(error("CODEX_RPC_CLOSED", "native server exited"));
    }
    serde_json::from_slice(&bytes).map_err(|e| error("CODEX_RPC_JSON", e))
}
fn ws_error(failure: tungstenite::Error) -> ControlError {
    let timeout = matches!(&failure, tungstenite::Error::Io(e) if matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock));
    error(
        if timeout {
            "CODEX_RPC_TIMEOUT"
        } else {
            "CODEX_RPC_SOCKET"
        },
        failure,
    )
}
impl Drop for Transport {
    fn drop(&mut self) {
        match self {
            Self::Process { child, .. } => {
                let _ = child.kill();
                let _ = child.wait();
            }
            Self::Daemon(socket) => {
                let _ = socket.close(None);
            }
        }
    }
}
