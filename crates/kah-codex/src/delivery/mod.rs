//! Host-only delegate delivery with durable receipts and native source IDs.
use crate::control::Control;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;
mod receipt;
pub use receipt::Receipt;

pub struct Message {
    pub session: String,
    pub client_id: String,
    pub text: String,
}
impl Message {
    pub fn new(session: &str, sender: &str, receipt: &str, text: &str) -> Result<Self, String> {
        if session.is_empty()
            || session.len() > 512
            || session.chars().any(char::is_control)
            || sender.is_empty()
            || sender.len() > 32
            || !sender
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
            || receipt.is_empty()
            || receipt.len() > 128
            || !receipt
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || text.trim().is_empty()
        {
            return Err("SEND_ARGUMENTS stage=delegate_channel".into());
        }
        let prefix = format!("[{sender}]");
        Ok(Self {
            session: session.into(),
            client_id: format!("kah-delegate:{sender}:{receipt}"),
            text: if text.starts_with(&prefix) {
                text.into()
            } else {
                format!("{prefix} {text}")
            },
        })
    }
    fn digest(&self) -> String {
        format!("{:x}", Sha256::digest(self.text.as_bytes()))
    }
    pub fn user(session: &str, receipt: &str, text: &str) -> Result<Self, String> {
        let mut message = Self::new(session, "operator", receipt, text)?;
        message.client_id = format!("kah-user:operator:{receipt}");
        message.text = text.into();
        Ok(message)
    }
}
#[derive(Debug)]
pub enum RpcError {
    Rejected,
    Unsupported,
    Unknown,
}
pub trait Rpc {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcError>;
}
impl Rpc for Control {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        Control::request(self, method, params).map_err(|e| {
            if e.code != "CODEX_RPC_REJECTED" {
                RpcError::Unknown
            } else if e.detail.contains("-32601") {
                RpcError::Unsupported
            } else {
                RpcError::Rejected
            }
        })
    }
}

pub fn send(base: &Path, message: &Message, rpc: &mut impl Rpc) -> Result<Value, String> {
    let mut receipt = Receipt::open(base, message)?;
    for _ in 0..2 {
        if delivered(rpc, message)? {
            receipt.save("accepted")?;
            return Ok(json!({"clientId":message.client_id,"status":"already_visible"}));
        }
        match receipt.state.as_str() {
            "accepted" => {
                return Ok(json!({"clientId":message.client_id,"status":"already_accepted"}));
            }
            "pending" => {
                return Err(format!(
                    "SEND_UNCERTAIN stage=delegate_channel receipt={}",
                    message.client_id
                ));
            }
            _ => (),
        }
        let active = active_turn(rpc, &message.session)?;
        let mut params = json!({"threadId":message.session,"clientUserMessageId":message.client_id,"input":[{"type":"text","text":message.text}]});
        let method = if let Some(id) = active {
            params["expectedTurnId"] = id.into();
            "turn/steer"
        } else {
            "turn/start"
        };
        receipt.save("pending")?;
        match rpc.request(method, params) {
            Ok(_) => {
                receipt.save("accepted")?;
                return Ok(json!({"clientId":message.client_id,"status":"accepted"}));
            }
            Err(RpcError::Rejected | RpcError::Unsupported) => receipt.save("rejected")?,
            Err(_) => {
                return Err(format!(
                    "SEND_UNCERTAIN stage=delegate_channel receipt={}",
                    message.client_id
                ));
            }
        }
    }
    Err("SEND_REJECTED stage=delegate_channel: receiver changed state or rejected input".into())
}
fn active_turn(rpc: &mut impl Rpc, session: &str) -> Result<Option<String>, String> {
    let page = match rpc.request(
        "thread/turns/list",
        json!({"threadId":session,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}),
    ) {
        Ok(page) => page,
        Err(RpcError::Unsupported) => {
            let read = legacy(rpc, session)?;
            json!({"data":read["thread"]["turns"]})
        }
        Err(_) => return Err("SEND_THREAD_STATE stage=delegate_channel".into()),
    };
    let turns = page["data"]
        .as_array()
        .ok_or("SEND_THREAD_SCHEMA stage=delegate_channel")?;
    Ok(turns
        .iter()
        .rev()
        .find(|t| t["status"] == "inProgress")
        .and_then(|t| t["id"].as_str())
        .map(str::to_owned))
}
fn legacy(rpc: &mut impl Rpc, session: &str) -> Result<Value, String> {
    rpc.request(
        "thread/read",
        json!({"threadId":session,"includeTurns":true}),
    )
    .map_err(|_| "SEND_LEGACY_READ stage=delegate_channel".into())
}
fn item_page(rpc: &mut impl Rpc, message: &Message, cursor: Value) -> Result<Value, String> {
    match rpc.request(
        "thread/items/list",
        json!({"threadId":message.session,"limit":25,"cursor":cursor,"sortDirection":"desc"}),
    ) {
        Ok(page) => Ok(page),
        Err(RpcError::Unsupported) => {
            let read = legacy(rpc, &message.session)?;
            let turns = read["thread"]["turns"]
                .as_array()
                .ok_or("SEND_LEGACY_SCHEMA stage=delegate_channel")?;
            let items = turns
                .iter()
                .flat_map(|t| t["items"].as_array().into_iter().flatten())
                .map(|item| json!({"item":item}))
                .collect::<Vec<_>>();
            Ok(json!({"data":items,"nextCursor":null}))
        }
        Err(_) => Err("SEND_RECEIPT_LOOKUP stage=delegate_channel".into()),
    }
}
fn delivered(rpc: &mut impl Rpc, message: &Message) -> Result<bool, String> {
    let mut cursor = Value::Null;
    for _ in 0..1000 {
        let page = item_page(rpc, message, cursor)?;
        let items = page["data"]
            .as_array()
            .ok_or("SEND_RECEIPT_SCHEMA stage=delegate_channel")?;
        if let Some(item) = items
            .iter()
            .map(|v| &v["item"])
            .find(|v| v["clientId"] == message.client_id)
        {
            let text = item["content"]
                .as_array()
                .ok_or("SEND_MESSAGE_SCHEMA stage=delegate_channel")?
                .iter()
                .filter_map(|v| v["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if text != message.text {
                return Err("SEND_RECEIPT_COLLISION stage=delegate_channel".into());
            }
            return Ok(true);
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            return Ok(false);
        }
    }
    Err("SEND_RECEIPT_BOUND stage=delegate_channel".into())
}
