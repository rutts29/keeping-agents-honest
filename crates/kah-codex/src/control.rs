//! Bounded native app-server requests over stdio or its local daemon socket.
use serde_json::{Value, json};
use std::{
    fmt,
    path::Path,
    time::{Duration, Instant},
};
mod transport;
use transport::Transport;

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
    transport: Transport,
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
        Self::initialize(Transport::process(binary, home, cwd)?, timeout)
    }
    pub fn connect_daemon(socket: &Path, timeout: Duration) -> Result<Self, ControlError> {
        Self::initialize(Transport::daemon(socket, timeout)?, timeout)
    }
    fn initialize(transport: Transport, timeout: Duration) -> Result<Self, ControlError> {
        let mut server = Self {
            transport,
            next: 1,
            timeout,
        };
        server.request("initialize",json!({"clientInfo":{"name":"kah_control","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}))?;
        server.transport.send(&json!({"method":"initialized"}))?;
        Ok(server)
    }
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, ControlError> {
        let id = self.next;
        self.next += 1;
        self.transport
            .send(&json!({"id":id,"method":method,"params":params}))?;
        let deadline = Instant::now() + self.timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(error("CODEX_RPC_TIMEOUT", method));
            }
            let message = self.transport.receive(deadline)?;
            // A message with a method comes from the server, whatever its id: a
            // notification is skipped, a request is declined under its own id.
            // Only a message without a method answers one of our requests.
            if message.get("method").is_some() {
                if let Some(server_id) = message.get("id") {
                    self.transport.send(&json!({"id":server_id,"error":{"code":-32601,"message":"this client handles no server requests"}}))?;
                }
                continue;
            }
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
}
