use super::*;

#[test]
fn configured_native_observer_flags_apply_to_cold_launch_and_resume() {
    for (provider, model) in [("codex", "gpt-6.1-sol"), ("claude", "claude-sonnet-5-5")] {
        for session in [None, Some("native-observer-session")] {
            let state: State = serde_json::from_value(json!({
                "worker":{"provider":provider,"session":"worker","project":"/tmp/project","pane":null,"socket":null},
                "observer":{"session":session,"pane":null,"name":"observer","workspace":"/tmp/observer","model":model,"effort":"low","transcript":null},
                "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
            })).unwrap();
            let args = arguments(&state, "Review this event").unwrap();
            assert_eq!(args.effort.as_deref(), Some("low"));
            assert!(
                args.native
                    .windows(2)
                    .any(|pair| pair == ["--model", model])
            );
            if let Some(id) = session {
                let flag = if provider == "claude" {
                    "--resume"
                } else {
                    "resume"
                };
                assert!(args.native.windows(2).any(|pair| pair == [flag, id]));
            }
            assert!(
                !args
                    .native
                    .iter()
                    .any(|arg| matches!(arg.as_str(), "exec" | "-p" | "--print"))
            );
        }
    }
}

#[test]
fn native_claude_scratch_is_inside_the_existing_writable_report_tree() {
    // Official anthropics/claude-code v2.1.292/CHANGELOG.md, 2.1.5 entry:
    // CLAUDE_CODE_TMPDIR is the native internal-temp override. This changes
    // kah-owned invocation settings only, not the user's settings or permissions.
    let root = std::env::temp_dir().join(format!(
        "kah-observer-scratch-{}",
        crate::relay_store::nonce()
    ));
    let mut settings = json!({"env":{"CLAUDE_CODE_SHELL_PREFIX":"fixed-prefix","CLAUDE_CODE_TMPDIR":"protected-runtime"}});
    let path = claude_scratch(&root, &mut settings).unwrap();
    assert_eq!(path, root.join("outbox/tmp"));
    assert!(path.is_dir());
    assert_eq!(
        settings["env"]["CLAUDE_CODE_TMPDIR"],
        path.to_str().unwrap()
    );
    assert_eq!(settings["env"]["CLAUDE_CODE_SHELL_PREFIX"], "fixed-prefix");
    std::fs::remove_dir_all(root).unwrap();
}
