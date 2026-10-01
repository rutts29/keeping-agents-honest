use kah_cli::herdr::{claude_session_report, session_report};
use serde_json::json;

#[test]
fn native_session_identity_is_reported_without_claiming_a_child_session() {
    let input = json!({"hook_event_name":"SessionStart","session_id":"native-id","transcript_path":"/history/rollout.jsonl","source":"resume"});
    let report = session_report(&input, "pane", None, 42).unwrap();
    assert_eq!(report["method"], "pane.report_agent_session");
    assert_eq!(report["params"]["agent"], "codex");
    assert_eq!(report["params"]["agent_session_id"], "native-id");
    assert_eq!(report["params"]["session_start_source"], "resume");
    assert!(session_report(&input, "pane", Some("parent-id"), 43).is_none());
    assert!(session_report(&json!({"hook_event_name":"Stop"}), "pane", None, 44).is_none());
}

#[test]
fn claude_identity_uses_the_native_protocol_without_claiming_subagents() {
    let mut input = json!({"hook_event_name":"SessionStart","session_id":"claude-id","transcript_path":"/history/claude-id.jsonl","source":"resume"});
    let report = claude_session_report(&input, "pane", 50).unwrap();
    assert_eq!(report["params"]["agent"], "claude");
    assert_eq!(report["params"]["source"], "herdr:claude");
    assert_eq!(
        report["params"]["agent_session_path"],
        "/history/claude-id.jsonl"
    );
    input["agent_id"] = "child".into();
    assert!(claude_session_report(&input, "pane", 51).is_none());
}
