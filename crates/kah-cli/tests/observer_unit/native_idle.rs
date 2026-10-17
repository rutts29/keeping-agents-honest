use super::*;

const SESSION: &str = "00000000-0000-4000-8000-100000000008";

fn claude(rows: &[Value]) -> String {
    rows.iter().map(|r| format!("{r}\n")).collect()
}

/// A transcript tail can show that the turn and its Stop hook ended,
/// while Herdr still shows the pane Working.
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
