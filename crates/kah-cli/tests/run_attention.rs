use kah_cli::run_pane::waiting_reason;
use serde_json::json;

#[test]
fn input_waits_for_the_bound_native_session_not_a_startup_shell_or_dialog() {
    use kah_cli::run_pane::accepts_input;
    let mut pane = json!({"agent":"codex","agent_status":"idle","agent_session":{"value":"saved"}});
    assert!(accepts_input(&pane, "codex", Some("saved")));
    assert!(!accepts_input(&pane, "claude", Some("saved")));
    assert!(!accepts_input(&pane, "codex", Some("different")));
    assert!(!accepts_input(&pane, "codex", None));
    for status in ["blocked", "unknown"] {
        pane["agent_status"] = status.into();
        assert!(!accepts_input(&pane, "codex", Some("saved")));
    }
    pane["agent_status"] = "working".into();
    assert!(accepts_input(&pane, "codex", Some("saved")));
    assert!(!accepts_input(&json!({}), "codex", Some("saved")));
}

#[test]
fn consent_and_slow_start_are_visible_but_active_work_is_not_a_wait() {
    assert!(waiting_reason(&json!({"agent_status":"blocked"}), true, 1).is_some());
    assert!(waiting_reason(&json!({"agent_status":"unknown"}), false, 15).is_some());
    assert!(waiting_reason(&json!({"agent_status":"unknown"}), false, 14).is_none());
    for status in ["working", "idle", "done", "unknown"] {
        assert!(waiting_reason(&json!({"agent_status":status}), true, 60).is_none());
    }
}
