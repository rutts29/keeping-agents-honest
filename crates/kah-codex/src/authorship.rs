//! Native user-role transport is not proof of human authorship.
//! Codex 0.159 contextual_user_message.rs uses these explicit runtime markers.
use kah_core::intent_history::SourceRole;

/// `delivered_by_agent` is trusted transport evidence, never model inference.
/// Bare Herdr terminal paste has no sender metadata in native thread/read;
/// callers must retain delivery provenance or use the named-agent prefix.
pub fn classify(text: &str, delivered_by_agent: bool) -> SourceRole {
    let text = text.trim_start();
    if delivered_by_agent
        || ["[kah-reviewer]", "[kah-builder]"]
            .iter()
            .any(|p| starts(text, p))
    {
        return SourceRole::Delegate;
    }
    if starts(text, "<send_user_message_question_reply>") {
        return SourceRole::User;
    }
    if [
        "<codex_internal_context",
        "<goal_context>",
        "<subagent_notification>",
        "<environment_context>",
        "# AGENTS.md instructions",
        "<skill>",
        "<turn_aborted>",
        "<recommended_plugins>",
    ]
    .iter()
    .any(|p| starts(text, p))
    {
        return SourceRole::SystemObservation;
    }
    SourceRole::User
}
fn starts(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
}
