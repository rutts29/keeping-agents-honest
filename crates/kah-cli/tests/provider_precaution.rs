use kah_cli::observer_review::delivery;

#[test]
fn native_precaution_picker_never_receives_automatic_keys() {
    // openai/codex rust-v0.162.1 tui/src/chatwidget/misalignment_policy.rs:125-183.
    let screen = "Chat paused as a precaution\nWe couldn’t confirm the agent was interpreting your instructions correctly.\n› 1. Review findings\n  2. New chat\n  3. Resume another chat\n\nenter to select";
    let error = delivery::safe_keyboard(screen).expect_err("native policy picker blocks all keys");
    assert!(error.to_string().starts_with("REVIEW_PROVIDER_BLOCKED"));
    assert!(
        delivery::safe_keyboard(&format!("{screen}\n› ")).is_ok(),
        "old quoted picker above a new composer is not active UI"
    );
    assert!(
        delivery::safe_keyboard(
            "Chat stopped as a precaution\n› 1. New chat\n2. Resume another chat\nEnter to select"
        )
        .is_err()
    );
    assert!(
        delivery::safe_keyboard(
            "Assistant explains: Chat paused as a precaution\n› Ask Codex to do anything"
        )
        .is_ok()
    );
}
