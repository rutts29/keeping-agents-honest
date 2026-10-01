use kah_cli::run_pane::waiting_reason;
use serde_json::json;

#[test]
fn consent_and_slow_start_are_visible_but_active_work_is_not_a_wait() {
    assert!(waiting_reason(&json!({"agent_status":"blocked"}), true, 1).is_some());
    assert!(waiting_reason(&json!({"agent_status":"unknown"}), false, 15).is_some());
    assert!(waiting_reason(&json!({"agent_status":"unknown"}), false, 14).is_none());
    for status in ["working", "idle", "done", "unknown"] {
        assert!(waiting_reason(&json!({"agent_status":status}), true, 60).is_none());
    }
}
