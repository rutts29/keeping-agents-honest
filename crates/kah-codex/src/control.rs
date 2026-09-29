//! Bounded native app-server control requests; no model selection or proxying.
use serde_json::{Value, json};
use std::{
    fmt,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

#[derive(Debug)]
pub struct ControlError {
    pub code: &'static str,
    pub detail: String,
}
impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} stage=codex_control: {}", self.code, self.detail)
    }
}
impl std::error::Error for ControlError {}
fn error(code: &'static str, detail: impl ToString) -> ControlError {
    ControlError {
        code,
        detail: detail.to_string(),
    }
}

pub struct Control {
    child: Child,
    stdin: ChildStdin,
    messages: Receiver<Result<Value, ControlError>>,
    next: u64,
    timeout: Duration,
}

impl Control {
    pub fn connect(
        binary: &Path,
        home: &Path,
        cwd: &Path,
        timeout: Duration,
    ) -> Result<Self, ControlError> {
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
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
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
        let mut server = Self {
            child,
            stdin,
            messages,
            next: 1,
            timeout,
        };
        server.request("initialize",json!({"clientInfo":{"name":"kah_control","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}))?;
        server.send(&json!({"method":"initialized"}))?;
        Ok(server)
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, ControlError> {
        let id = self.next;
        self.next += 1;
        self.send(&json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + self.timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(error("CODEX_RPC_TIMEOUT", method));
            }
            let message = self
                .messages
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|e| {
                    error(
                        if e == RecvTimeoutError::Timeout {
                            "CODEX_RPC_TIMEOUT"
                        } else {
                            "CODEX_RPC_CLOSED"
                        },
                        method,
                    )
                })??;
            if message.get("id").is_none() {
                continue;
            }
            if message["id"] != id {
                return Err(error("CODEX_RPC_ID", method));
            }
            if let Some(rejection) = message.get("error") {
                return Err(error(
                    "CODEX_RPC_REJECTED",
                    format!("{method}: {rejection}"),
                ));
            }
            return message
                .get("result")
                .cloned()
                .ok_or_else(|| error("CODEX_RPC_RESULT", method));
        }
    }

    fn send(&mut self, value: &Value) -> Result<(), ControlError> {
        writeln!(self.stdin, "{value}").map_err(|e| error("CODEX_RPC_WRITE", e))
    }
}

fn read_message(reader: &mut impl BufRead) -> Result<Value, ControlError> {
    const LIMIT: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    let count = reader
        .take(LIMIT + 1)
        .read_until(b'\n', &mut bytes)
        .map_err(|e| error("CODEX_RPC_READ", e))?;
    if count == 0 {
        return Err(error("CODEX_RPC_CLOSED", "native server exited"));
    }
    if bytes.len() as u64 > LIMIT {
        return Err(error("CODEX_RPC_LIMIT", "native response exceeds 8 MiB"));
    }
    serde_json::from_slice(&bytes).map_err(|e| error("CODEX_RPC_JSON", e))
}

impl Drop for Control {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
