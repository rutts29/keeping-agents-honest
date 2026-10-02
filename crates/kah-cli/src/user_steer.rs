//! User-authorized host relay for native sessions without a control socket.
use crate::{Result, relay_store, relay_transport::herdr};
use kah_codex::delivery::{Message as Delivery, Receipt};
use kah_core::run_store::RunStore;
use kah_observer::relayed_user::Message;
use serde_json::{Value, json};
use std::path::Path;

pub fn deliver(session: &str, text: &str, receipt: &str) -> Result<Value> {
    let endpoint = relay_store::endpoint(session)?;
    let pane = endpoint
        .pane
        .as_deref()
        .ok_or("STEER_HERDR_PANE stage=delivery: receiver has no Herdr pane")?;
    let socket = endpoint
        .socket
        .as_deref()
        .ok_or("STEER_HERDR_SOCKET stage=delivery: receiver has no recorded Herdr server")?;
    let view = herdr(socket, &["pane", "get", pane])?;
    if view["result"]["pane"]["agent"] != endpoint.provider
        || relay_store::pane_endpoint(pane, socket)?.session != session
    {
        return Err(
            "STEER_PANE_CHANGED stage=delivery: receiver no longer owns its recorded pane".into(),
        );
    }
    let relayer = relayer()?;
    let envelope = Message::new(&relayer, receipt, text)?;
    let mut delivery = Delivery::user(session, receipt, &envelope.wire())?;
    delivery.client_id = format!("kah-user-relayed:{relayer}:{receipt}");
    let mut saved = Receipt::open(&RunStore::default_state_root()?, &delivery)?;
    if saved.state == "accepted" {
        return Ok(
            json!({"status":"already_submitted","transport":"herdr","origin":"relayed","relayer":relayer,"receipt":receipt}),
        );
    }
    if saved.state == "pending" {
        return Err(format!(
            "SEND_UNCERTAIN stage=delivery: receipt {receipt}; inspect the receiver before retrying"
        )
        .into());
    }
    saved.save("pending")?;
    herdr(socket, &["agent", "prompt", pane, &delivery.text])?;
    saved.save("accepted")?;
    Ok(
        json!({"status":"submitted","transport":"herdr","origin":"relayed","relayer":relayer,"receipt":receipt,"receiverAcknowledgmentRequired":true}),
    )
}

fn relayer() -> Result<String> {
    let (Ok(pane), Some(socket)) = (
        std::env::var("HERDR_PANE_ID"),
        std::env::var_os("HERDR_SOCKET_PATH"),
    ) else {
        return Ok("operator".into());
    };
    let view = herdr(Path::new(&socket), &["agent", "get", &pane]);
    Ok(view
        .ok()
        .and_then(|v| v["result"]["agent"]["name"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "operator".into()))
}
