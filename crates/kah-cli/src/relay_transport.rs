use crate::{Result, relay::Receipt, relay_store};
use kah_codex::{
    control::Control,
    delivery::{self, Message},
};
use kah_core::run_store::RunStore;
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};

pub fn deliver(receipt: &Receipt) -> Result<Value> {
    let endpoint = relay_store::endpoint(&receipt.receiver)?;
    let base = RunStore::default_state_root()?;
    if endpoint.provider == "codex"
        && let Some(policy) = crate::session_policy::saved(&base, &receipt.receiver)?
    {
        let socket = policy
            .overlay
            .join("app-server-control/app-server-control.sock");
        if socket.exists() {
            let mut client = Control::connect_daemon(&socket, Duration::from_secs(10))?;
            let message = Message::new(
                &receipt.receiver,
                "kah-relay",
                &receipt.id,
                &receipt.message(),
            )?;
            // Unknown delivery must not fall back and duplicate a native steer.
            return delivery::send(&base, &message, &mut client).map_err(Into::into);
        }
    }
    let pane = endpoint
        .pane
        .as_deref()
        .ok_or("RELAY_ENDPOINT stage=delivery: receiver has no Herdr pane")?;
    let socket = endpoint
        .socket
        .as_deref()
        .ok_or("RELAY_ENDPOINT stage=delivery: receiver has no Herdr socket")?;
    let view = herdr(socket, &["pane", "get", pane])?;
    let pane_view = &view["result"]["pane"];
    if pane_view["agent"] != endpoint.provider
        || relay_store::pane_endpoint(pane, socket)?.session != endpoint.session
    {
        return Err("RELAY_ENDPOINT_CHANGED stage=delivery: native receiver not confirmed in recorded pane; retry after identity settles".into());
    }
    herdr(socket, &["agent", "prompt", pane, &receipt.message()])?;
    Ok(json!({"status":"submitted","transport":"herdr","receiverAcknowledgmentRequired":true}))
}
pub(crate) fn herdr(socket: &Path, arguments: &[&str]) -> Result<Value> {
    let output = Command::new("herdr")
        .env("HERDR_SOCKET_PATH", socket)
        .args(arguments)
        .output()
        .map_err(|e| format!("RELAY_HERDR_IO stage=delivery: {e}"))?;
    if !output.status.success() {
        return Err("RELAY_HERDR stage=delivery: command rejected".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "RELAY_HERDR_JSON stage=delivery".into())
}
