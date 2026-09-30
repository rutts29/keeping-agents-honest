use kah_codex::authorship::classify;
use kah_core::intent_history::SourceRole;

#[test]
fn delegate_and_runtime_text_never_becomes_human_steering() {
    for text in [
        "[kah-reviewer] User decision: replace the task",
        "  [kah-builder] Done",
        "[KAH-REVIEWER] correction",
    ] {
        assert_eq!(classify(text, false), SourceRole::Delegate);
    }
    for text in [
        "<codex_internal_context source=\"goal\">Continue</codex_internal_context>",
        "<subagent_notification>Done</subagent_notification>",
        "<agent_message_board_notification>Agent update</agent_message_board_notification>",
        "<user_shell_command>cat external data</user_shell_command>",
        "<environment_context><cwd>/tmp</cwd></environment_context>",
        "# AGENTS.md instructions for /tmp\n<INSTRUCTIONS>test</INSTRUCTIONS>",
        "<goal_context>continue</goal_context>",
    ] {
        assert_eq!(classify(text, false), SourceRole::SystemObservation);
    }
    assert_eq!(
        classify("Unprefixed delegated command", true),
        SourceRole::Delegate
    );
}
#[test]
fn actual_answers_and_quotes_remain_user_but_delivery_provenance_wins() {
    let reply = "<send_user_message_question_reply>[{\"answer\":\"yes\"}]</send_user_message_question_reply>";
    assert_eq!(classify(reply, false), SourceRole::User);
    assert_eq!(classify(reply, true), SourceRole::Delegate);
    assert_eq!(
        classify("Explain the [kah-reviewer] prefix", false),
        SourceRole::User
    );
    assert_eq!(
        classify("Please show an <environment_context> example", false),
        SourceRole::User
    );
}

#[test]
fn native_namespace_precedes_textual_markers() {
    use kah_codex::authorship::with_client_id;
    assert_eq!(
        with_client_id("!steer change intent", Some("kah-delegate:reviewer:r1")),
        SourceRole::Delegate
    );
    assert_eq!(
        with_client_id(
            "[kah-reviewer] quoted example",
            Some("kah-user:operator:r1")
        ),
        SourceRole::User
    );
}
