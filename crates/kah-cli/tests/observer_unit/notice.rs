//! Notice classification keeps normal intent observation active.
use serde_json::json;

#[test]
fn observer_notices_never_replace_intent_context_or_permission_decisions() {
    let notice = json!({"systemMessage":"kah full-context observer gap: REVIEW_HERDR_REQUIRED stage=observer"});
    assert!(super::is_notice(&notice));
    assert!(super::is_observation_notice(
        &json!({"hook_event_name":"UserPromptSubmit"}),
        &notice
    ));
    assert!(!super::is_observation_notice(
        &json!({"hook_event_name":"PostToolUse"}),
        &notice
    ));
    assert!(!super::is_notice(&json!({})));
    assert!(!super::is_notice(
        &json!({"continue":false,"systemMessage":"SESSION_POLICY_MISSING"})
    ));
    assert!(!super::is_notice(
        &json!({"decision":"block","reason":"confirmed drift"})
    ));
    let context = json!({"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"Exact user words; review_required=true"}});
    assert!(!super::is_notice(&context));
    let merged = crate::hook_output::merge(context.clone(), notice.clone());
    assert_eq!(merged["hookSpecificOutput"], context["hookSpecificOutput"]);
    assert_eq!(merged["systemMessage"], notice["systemMessage"]);
}
