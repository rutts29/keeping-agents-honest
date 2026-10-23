//! Delivery looks the message up in the whole item history, whatever its
//! length, and a durable receipt answers without any history lookup.
use kah_codex::delivery::{self, Message, Rpc, RpcError};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const PAGES: usize = 1_001;

/// A session whose history has `PAGES` pages, newest first, of one item each.
struct Paged {
    visible_on_page: Option<usize>,
    text: String,
    item_lists: usize,
    submissions: usize,
    cursor_loop: bool,
    refuse_history: bool,
}

impl Paged {
    fn new(message: &Message) -> Self {
        Self {
            visible_on_page: None,
            text: message.text.clone(),
            item_lists: 0,
            submissions: 0,
            cursor_loop: false,
            refuse_history: false,
        }
    }
}

impl Rpc for Paged {
    fn request(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "thread/items/list" => {
                assert!(
                    !self.refuse_history,
                    "a durable receipt needs no history lookup"
                );
                self.item_lists += 1;
                let page = params["cursor"].as_u64().unwrap_or(0) as usize;
                let item = if self.visible_on_page == Some(page) {
                    json!({"item":{"clientId":"kah-delegate:reviewer:r1","content":[{"text":self.text}]}})
                } else {
                    json!({"item":{"clientId":format!("other-{page}"),"content":[{"text":"x"}]}})
                };
                let next = if self.cursor_loop {
                    json!(page)
                } else if page + 1 < PAGES {
                    json!(page + 1)
                } else {
                    Value::Null
                };
                Ok(json!({"data":[item],"nextCursor":next}))
            }
            "thread/turns/list" => Ok(json!({"data":[]})),
            "turn/start" | "turn/steer" => {
                self.submissions += 1;
                Ok(json!({}))
            }
            _ => panic!("unexpected method {method}"),
        }
    }
}

fn base() -> PathBuf {
    std::env::temp_dir().join(format!(
        "kah-delivery-pages-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn message() -> Message {
    Message::new("s", "reviewer", "r1", "Please review").unwrap()
}

#[test]
fn a_message_absent_from_a_long_history_is_sent_once() {
    let (base, message) = (base(), message());
    let mut rpc = Paged::new(&message);
    let sent = delivery::send(&base, &message, &mut rpc).unwrap();
    assert_eq!(sent["status"], "accepted");
    assert_eq!(rpc.submissions, 1);
    assert_eq!(
        rpc.item_lists, PAGES,
        "the whole history was read, to its last page"
    );
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_message_on_a_late_page_is_found_not_sent_again() {
    let (base, message) = (base(), message());
    let mut rpc = Paged::new(&message);
    rpc.visible_on_page = Some(PAGES - 1);
    let sent = delivery::send(&base, &message, &mut rpc).unwrap();
    assert_eq!(sent["status"], "already_visible");
    assert_eq!(rpc.submissions, 0);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn an_accepted_receipt_answers_without_reading_the_history() {
    let (base, message) = (base(), message());
    let mut rpc = Paged::new(&message);
    delivery::send(&base, &message, &mut rpc).unwrap();
    rpc.refuse_history = true;
    let again = delivery::send(&base, &message, &mut rpc).unwrap();
    assert_eq!(again["status"], "already_accepted");
    assert_eq!(rpc.submissions, 1);
    let changed = Message::new("s", "reviewer", "r1", "Different text").unwrap();
    assert!(
        delivery::send(&base, &changed, &mut rpc)
            .unwrap_err()
            .starts_with("SEND_RECEIPT_COLLISION")
    );
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_cursor_that_never_advances_is_an_error_not_an_endless_walk() {
    let (base, message) = (base(), message());
    let mut rpc = Paged::new(&message);
    rpc.cursor_loop = true;
    let error = delivery::send(&base, &message, &mut rpc).unwrap_err();
    assert!(error.starts_with("SEND_RECEIPT_CURSOR"), "{error}");
    assert_eq!(rpc.submissions, 0);
    std::fs::remove_dir_all(base).unwrap();
}
