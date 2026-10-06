use kah_codex::delivery::{self, Message, Rpc, RpcError};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
struct Fake {
    writes: usize,
    visible: bool,
    active: bool,
    reject_once: bool,
    transport_error: bool,
    message: String,
}
impl Rpc for Fake {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "thread/items/list" => Ok(
                json!({"data":if self.visible {vec![json!({"item":{"clientId":"kah-delegate:reviewer:r1","content":[{"text":self.message}]}})]}else{vec![]},"nextCursor":null}),
            ),
            "thread/turns/list" => Ok(
                json!({"data":if self.active {vec![json!({"id":"t1","status":"inProgress"})]}else{vec![]}}),
            ),
            "turn/start" | "turn/steer" => {
                assert_eq!(params["clientUserMessageId"], "kah-delegate:reviewer:r1");
                if self.reject_once {
                    self.reject_once = false;
                    self.active = false;
                    return Err(RpcError::Rejected);
                }
                self.writes += 1;
                if self.transport_error {
                    return Err(RpcError::Unknown);
                }
                self.visible = true;
                self.message = params["input"][0]["text"].as_str().unwrap().into();
                Ok(json!({}))
            }
            _ => panic!("unexpected method"),
        }
    }
}
fn fixture() -> (PathBuf, Message, Fake) {
    let base = std::env::temp_dir().join(format!(
        "kah-delivery-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    (
        base,
        Message::new("s", "reviewer", "r1", "Please review").unwrap(),
        Fake {
            writes: 0,
            visible: false,
            active: true,
            reject_once: false,
            transport_error: false,
            message: String::new(),
        },
    )
}
#[test]
fn duplicate_receipt_after_restart_does_not_send_twice() {
    let (base, message, mut rpc) = fixture();
    delivery::send(&base, &message, &mut rpc).unwrap();
    delivery::send(&base, &message, &mut rpc).unwrap();
    assert_eq!(rpc.writes, 1);
    let changed = Message::new("s", "reviewer", "r1", "Different text").unwrap();
    assert!(
        delivery::send(&base, &changed, &mut rpc)
            .unwrap_err()
            .contains("COLLISION")
    );
    std::fs::remove_dir_all(base).unwrap();
}
#[test]
fn active_to_idle_race_rechecks_receipt_and_uses_one_accepted_submission() {
    let (base, message, mut rpc) = fixture();
    rpc.reject_once = true;
    delivery::send(&base, &message, &mut rpc).unwrap();
    assert_eq!(rpc.writes, 1);
    std::fs::remove_dir_all(base).unwrap();
}
#[test]
fn lost_ack_never_blindly_resends_but_recovers_a_visible_receipt() {
    let (base, message, mut rpc) = fixture();
    rpc.transport_error = true;
    assert!(delivery::send(&base, &message, &mut rpc).is_err());
    assert!(
        delivery::send(&base, &message, &mut rpc)
            .unwrap_err()
            .contains("UNCERTAIN")
    );
    assert_eq!(rpc.writes, 1);
    rpc.visible = true;
    rpc.message = message.text.clone();
    delivery::send(&base, &message, &mut rpc).unwrap();
    assert_eq!(rpc.writes, 1);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn accepted_receipt_read_is_non_mutating_and_exact_payload_bound() {
    use kah_codex::delivery::Receipt;
    let (base, message, _) = fixture();
    assert!(!Receipt::accepted(&base, &message).unwrap());
    assert!(!base.exists());
    let mut receipt = Receipt::open(&base, &message).unwrap();
    receipt.save("pending").unwrap();
    assert!(!Receipt::accepted(&base, &message).unwrap());
    receipt.save("accepted").unwrap();
    assert!(Receipt::accepted(&base, &message).unwrap());
    let changed = Message::new("s", "reviewer", "r1", "different payload").unwrap();
    assert!(
        Receipt::accepted(&base, &changed)
            .unwrap_err()
            .starts_with("SEND_RECEIPT_COLLISION")
    );
    let foreign = Message::new("other", "reviewer", "r1", "Please review").unwrap();
    assert!(!Receipt::accepted(&base, &foreign).unwrap());
    drop(receipt);
    std::fs::remove_dir_all(base).unwrap();
}
