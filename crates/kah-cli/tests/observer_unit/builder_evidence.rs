use super::*;
use kah_observer::review_evidence::{Document, NativeRecord};
fn document(kind: &str, id: &str) -> Document {
    let mut document = Document::new(
        "transcript",
        "worker_transcript",
        "native:codex:worker",
        "receipt-marker",
    );
    document.native_records = vec![NativeRecord {
        start_byte: 0,
        end_byte: document.text.len(),
        context: json!({"record_type":kind,"record_id":id}),
    }];
    document
}
#[test]
fn response_uses_host_native_provenance_and_ignores_old_messages_and_tool_output() {
    // Upstream checked first: openai/codex rust-v0.160.0,
    // codex-rs/protocol/src/items.rs TurnItem::AgentMessage; kah capture/rendering
    // attaches provenance from native JSON, never marker-looking tool text.
    let before = document("agentMessage", "old");
    assert!(response(&before, &document("agentMessage", "new"), "receipt-marker").is_some());
    assert!(response(&before, &document("agentMessage", "old"), "receipt-marker").is_none());
    assert!(
        response(
            &before,
            &document("commandExecution", "new"),
            "receipt-marker"
        )
        .is_none()
    );
    let mut synthetic = document("agentMessage", "new");
    synthetic.native_records[0].context["is_meta"] = true.into();
    assert!(response(&before, &synthetic, "receipt-marker").is_none());
    synthetic.native_records[0].context["is_meta"] = false.into();
    synthetic.native_records[0].context["is_sidechain"] = true.into();
    assert!(response(&before, &synthetic, "receipt-marker").is_none());
}

#[test]
fn missing_receipt_in_a_completed_new_primary_reply_remains_visible_evidence() {
    let before = document("agentMessage", "old");
    let mut current = document("agentMessage", "new");
    current.text = "receipt-markerExisting test and SHA receipts follow.".into();
    current.native_records[0].start_byte = "receipt-marker".len();
    current.native_records[0].end_byte = current.text.len();
    current.native_records.insert(
        0,
        NativeRecord {
            start_byte: 0,
            end_byte: "receipt-marker".len(),
            context: json!({"record_type":"userMessage","record_id":"ask"}),
        },
    );
    assert!(response(&before, &current, "receipt-marker").is_none());
    assert!(new_primary_reply(&before, &current, "receipt-marker").is_some());
    assert!(
        new_primary_reply(
            &before,
            &document("commandExecution", "new"),
            "receipt-marker"
        )
        .is_none()
    );
    assert!(
        new_primary_reply(&before, &document("agentMessage", "old"), "receipt-marker").is_none()
    );
}

#[test]
fn an_assistant_reply_before_the_ask_snapshot_cannot_end_the_evidence_wait() {
    let before = document("agentMessage", "already-finished-after-checkpoint");
    let current = before.clone();
    assert!(new_primary_reply(&before, &current, "receipt-marker").is_none());
    assert!(response(&before, &current, "receipt-marker").is_none());
    assert!(
        response(
            &before,
            &document("agentMessage", "reply-after-ask"),
            "receipt-marker"
        )
        .is_some()
    );
}

#[test]
fn claude_mid_turn_queued_ask_bounds_a_reply_that_omits_the_full_receipt() {
    // Claude Code delivered the evidence request during a running
    // turn as a queued_command attachment; the reply cited a shortened id.
    let receipt = "followup-0123456789abcdef-builder";
    let enqueued = json!({"type":"queue-operation","operation":"enqueue",
        "content":format!("[kah-observer] kah evidence request {receipt}")});
    let ask = json!({"type":"attachment","uuid":"ask","isSidechain":false,
        "attachment":{"type":"queued_command","commandMode":"prompt",
        "prompt":format!("[kah-observer] kah evidence request {receipt}: reply with this receipt")}});
    let reply = json!({"type":"assistant","uuid":"reply","isSidechain":false,
        "message":{"role":"assistant","content":[{"type":"text","text":"Receipts for followup-01234567 follow."}]}});
    let native = |rows: Value| {
        crate::observer_review::capture::native_document("claude", "worker", &rows).unwrap()
    };
    let before = native(json!([enqueued.clone(), ask.clone()]));
    let current = native(json!([enqueued.clone(), ask, reply.clone()]));
    assert!(response(&before, &current, receipt).is_none());
    assert!(new_primary_reply(&before, &current, receipt).is_some());
    // Enqueued but never delivered input is not an ask.
    let undelivered = native(json!([enqueued, reply]));
    assert!(new_primary_reply(&before, &undelivered, receipt).is_none());
}
