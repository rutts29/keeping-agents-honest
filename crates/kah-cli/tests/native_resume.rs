use kah_cli::native_resume;
use serde_json::json;
use std::path::Path;

#[test]
fn empty_or_partial_claude_history_is_not_a_resumable_user_task() {
    assert!(!native_resume::claude_has_user("", "s", Path::new("/project")).unwrap());
    let row=json!({"type":"user","uuid":"u","sessionId":"s","cwd":"/project","message":{"role":"user","content":"task"}}).to_string();
    assert!(
        native_resume::claude_has_user(&format!("{row}\n{{incomplete"), "s", Path::new("/project"))
            .unwrap()
    );
}

#[test]
fn only_native_missing_thread_errors_allow_an_empty_restart() {
    use kah_codex::control::ControlError;
    let missing = ControlError {
        code: "CODEX_RPC_REJECTED",
        detail: "thread/read: {\"code\":-32600,\"message\":\"thread not loaded: saved\"}".into(),
    };
    assert!(native_resume::missing_thread(&missing, "saved"));
    assert!(!native_resume::missing_thread(&missing, "different"));
    let failed = ControlError {
        code: "CODEX_RPC_REJECTED",
        detail: "thread/read: {\"code\":-32600,\"message\":\"permission denied\"}".into(),
    };
    assert!(!native_resume::missing_thread(&failed, "saved"));
}
