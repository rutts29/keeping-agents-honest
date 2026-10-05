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
