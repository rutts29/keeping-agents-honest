//! Delegate fallback uses native Herdr prompt, preserving the delivery receipt.
use crate::{Result, relay_store, relay_transport::herdr};
use kah_codex::delivery::{Message, Receipt};
use kah_core::run_store::RunStore;
use serde_json::{Value, json};

pub(super) fn deliver(message: &Message) -> Result<Value> {
    let endpoint = relay_store::endpoint(&message.session)?;
    let pane = endpoint
        .pane
        .as_deref()
        .ok_or("SEND_HERDR_PANE stage=delivery")?;
    let socket = endpoint
        .socket
        .as_deref()
        .ok_or("SEND_HERDR_SOCKET stage=delivery")?;
    let view = herdr(socket, &["pane", "get", pane])?;
    if view["result"]["pane"]["agent"] != endpoint.provider
        || relay_store::pane_endpoint(pane, socket)?.session != message.session
    {
        return Err(
            "SEND_PANE_CHANGED stage=delivery: receiver no longer owns its recorded pane".into(),
        );
    }
    let mut receipt = Receipt::open(&RunStore::default_state_root()?, message)?;
    match receipt.state.as_str() {
        "accepted" => {
            return Ok(
                json!({"clientId":message.client_id,"status":"already_accepted","transport":"herdr"}),
            );
        }
        "pending" => {
            return Err(format!(
                "SEND_UNCERTAIN stage=delegate_channel receipt={}",
                message.client_id
            )
            .into());
        }
        _ => (),
    }
    receipt.save("pending")?;
    // Herdr 0.9.3 natively prompts the recognized agent and returns JSON:
    // https://github.com/herdrdev/herdr/blob/v0.9.3/src/cli/agent.rs#L777-L851
    // Keep Message::new's delegate prefix; never create a relayed-user envelope.
    herdr(socket, &["agent", "prompt", pane, &message.text])?;
    receipt.save("accepted")?;
    Ok(
        json!({"clientId":message.client_id,"status":"submitted","transport":"herdr","receiverAcknowledgmentRequired":true}),
    )
}
