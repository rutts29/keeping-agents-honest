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
