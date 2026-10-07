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
    current.text = "Existing test and SHA receipts follow.".into();
    current.native_records[0].end_byte = current.text.len();
    assert!(response(&before, &current, "receipt-marker").is_none());
    assert!(new_primary_reply(&before, &current).is_some());
    assert!(new_primary_reply(&before, &document("commandExecution", "new")).is_none());
    assert!(new_primary_reply(&before, &document("agentMessage", "old")).is_none());
}
