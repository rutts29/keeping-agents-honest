use super::*;
use kah_core::intent_history::{GoalRevision, History, SourceProvider, SourceRef, SourceRole};
use kah_core::projection::project;

#[test]
fn projection_preserves_user_provenance_and_redacts_private_session_locator() {
    let event = HookInput::parse(r#"{"session_id":"private-session","cwd":"/fixture","hook_event_name":"UserPromptSubmit","model":"chosen-model","prompt":"Keep scope"}"#).unwrap();
    let source = SourceRef::new(
        SourceProvider::Codex,
        "private-session",
        "user-1",
        SourceRole::User,
        "sha256:fixture",
    );
    let history = History::new(GoalRevision::new(1, "Keep scope", vec![source])).unwrap();
    let projection = project(&history, 16384).unwrap();
    let output = projection_output(&event, &projection).unwrap();
    assert!(!output.contains("private-session"));
    let document: Value = serde_json::from_str(&output).unwrap();
    assert_eq!(
        document["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
    assert!(
        document["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("Keep scope")
    );
    assert!(!document.to_string().contains("decision"));
}

#[test]
fn invalid_hook_input_has_distinct_json_and_required_field_errors() {
    assert!(
        HookInput::parse("not JSON")
            .unwrap_err()
            .starts_with("HOOK_JSON")
    );
    assert!(
        HookInput::parse(
            r#"{"session_id":"","cwd":"/fixture","hook_event_name":"Stop","model":"chosen"}"#
        )
        .unwrap_err()
        .starts_with("HOOK_FIELD_EMPTY")
    );
}
