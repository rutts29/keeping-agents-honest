use super::*;

const SESSION: &str = "00000000-0000-4000-8000-000000000001";

fn claude(rows: &[Value]) -> String {
    rows.iter().map(|r| format!("{r}\n")).collect()
}

/// A synthetic transcript tail where the turn and Stop hook ended,
/// while the pane still reports Working.
fn ended_turn() -> Vec<Value> {
    vec![
        json!({"type":"user","sessionId":SESSION,"message":{"role":"user","content":"kah review request review-1 ..."}}),
        json!({"type":"assistant","sessionId":SESSION,"message":{"content":[{"type":"text","text":"Acknowledged"}]}}),
        json!({"type":"attachment","sessionId":SESSION,"attachment":{"type":"hook_success","hookName":"Stop"}}),
        json!({"type":"system","subtype":"stop_hook_summary","sessionId":SESSION,"hookCount":1}),
        json!({"type":"system","subtype":"turn_duration","sessionId":SESSION,"durationMs":312724}),
        json!({"type":"last-prompt","sessionId":SESSION}),
    ]
}

#[test]
fn a_claude_turn_with_turn_duration_is_ended_even_when_herdr_says_working() {
    assert!(turn_ended("claude", &claude(&ended_turn()), SESSION));
}

#[test]
fn a_claude_turn_still_running_or_restarted_is_not_ended() {
    let mut rows = ended_turn();
    rows.truncate(3);
    assert!(!turn_ended("claude", &claude(&rows), SESSION), "mid-turn");
    let mut rows = ended_turn();
    rows.push(
        json!({"type":"user","sessionId":SESSION,"message":{"role":"user","content":"next"}}),
    );
    assert!(!turn_ended("claude", &claude(&rows), SESSION), "new turn");
    let text = claude(&ended_turn());
    assert!(
        !turn_ended("claude", text.trim_end(), SESSION),
        "partial write"
    );
    assert!(!turn_ended("claude", "", SESSION), "no turn yet");
}

#[test]
fn a_codex_turn_ends_with_task_complete_until_new_input() {
    let started = json!({"type":"event_msg","payload":{"type":"task_started"}});
    let complete = json!({"type":"event_msg","payload":{"type":"task_complete"}});
    let user =
        json!({"type":"response_item","payload":{"type":"message","role":"user","content":[]}});
    let text = |rows: &[&Value]| rows.iter().map(|r| format!("{r}\n")).collect::<String>();
    assert!(turn_ended(
        "codex",
        &text(&[&started, &user, &complete]),
        ""
    ));
    assert!(!turn_ended("codex", &text(&[&started, &user]), ""));
    assert!(!turn_ended("codex", &text(&[&complete, &user]), ""));
    assert!(!turn_ended("codex", &text(&[&complete, &started]), ""));
}

#[test]
fn the_transcript_proof_starts_one_delivery_per_request() {
    let directory =
        std::env::temp_dir().join(format!("kah-native-idle-{}", crate::relay_store::nonce()));
    assert!(unclaimed(&directory, "review-1"));
    record(&directory, "review-1").unwrap();
    assert!(!unclaimed(&directory, "review-1"));
    assert!(unclaimed(&directory, "review-2"));
    std::fs::remove_dir_all(directory).unwrap();
}

const MARKER: &str = "kah review request review-1 ";

#[test]
fn a_receipt_from_a_turn_that_ended_without_a_report_does_not_count_for_a_redelivery() {
    // A receipt from a turn that ended without a report must not satisfy
    // a later redelivery.
    let text = claude(&ended_turn());
    assert!(!open_receipt(&text, "claude", SESSION, MARKER));
}

#[test]
fn a_receipt_counts_while_its_turn_is_open_or_after_a_new_delivery() {
    let mut rows = ended_turn();
    rows.truncate(1);
    assert!(
        open_receipt(&claude(&rows), "claude", SESSION, MARKER),
        "just received"
    );
    let mut rows = ended_turn();
    rows.truncate(2);
    assert!(
        open_receipt(&claude(&rows), "claude", SESSION, MARKER),
        "mid-turn"
    );
    let mut rows = ended_turn();
    rows.push(ended_turn()[0].clone());
    assert!(
        open_receipt(&claude(&rows), "claude", SESSION, MARKER),
        "redelivered"
    );
    let other = "kah review request review-2 ";
    assert!(!open_receipt(
        &claude(&ended_turn()[..1]),
        "claude",
        SESSION,
        other
    ));
}

#[test]
fn a_stale_receipt_no_longer_blocks_the_transcript_override_or_the_deadline() {
    use crate::observer_review::missing_report::{UNDELIVERED_MS, undelivered_expired};
    let text = claude(&ended_turn());
    let confirmed = open_receipt(&text, "claude", SESSION, MARKER);
    // warm() offers the transcript override only when nothing is confirmed.
    assert!(!confirmed && turn_ended("claude", &text, SESSION));
    assert!(undelivered_expired(Some(0), UNDELIVERED_MS, confirmed));
}
