use kah_cli::unattended_args::RunArguments;

#[test]
fn run_options_preserve_native_model_choices_and_require_verification() {
    let args = [
        "--model",
        "chosen",
        "--effort",
        "high",
        "--domain",
        "research",
        "--budget-tokens",
        "99",
        "--time-limit",
        "30",
        "--test",
        "cargo test",
        "fix bug",
    ]
    .map(str::to_owned)
    .to_vec();
    let parsed = RunArguments::parse(args).unwrap();
    assert_eq!(parsed.task, "fix bug");
    assert_eq!(parsed.model.as_deref(), Some("chosen"));
    assert_eq!(parsed.effort.as_deref(), Some("high"));
    assert_eq!(parsed.limits.tokens, Some(99));
    assert_eq!(parsed.test, "cargo test");
    for args in [
        vec!["task"],
        vec!["--test", "true", "--budget-tokens", "-1", "task"],
        vec!["--agent", "unknown", "--test", "true", "task"],
    ] {
        assert!(RunArguments::parse(args.into_iter().map(str::to_owned).collect()).is_err());
    }
    let native = RunArguments::parse(
        ["--agent", "claude", "--online", "--test", "true", "task"]
            .map(str::to_owned)
            .to_vec(),
    )
    .unwrap();
    assert_eq!(native.agent.as_deref(), Some("claude"));
    assert!(native.online);
}

#[test]
fn the_retired_fetch_broker_is_rejected_before_creating_a_run() {
    let error = RunArguments::parse(
        ["--allow-host", "example.com", "--test", "true", "task"]
            .map(str::to_owned)
            .to_vec(),
    )
    .err()
    .expect("retired broker accepted");
    assert!(error.starts_with("RUN_NETWORK_POLICY stage=arguments:"));
}

#[test]
fn native_service_tier_override_is_explicit_and_other_execution_overrides_stay_denied() {
    // openai/codex rust-v0.160.0 core/src/config/config_tests.rs:
    // default_service_tier_override_uses_default_request_value.
    let parse = |setting: &str| {
        RunArguments::parse(
            ["-c", setting, "--test", "true", "task"]
                .map(str::to_owned)
                .to_vec(),
        )
    };
    assert_eq!(
        parse("service_tier=\"default\"").unwrap().codex_config,
        vec!["service_tier=\"default\""]
    );
    assert!(parse("sandbox_mode=\"danger-full-access\"").is_err());
    assert!(parse("exec_server.program=\"host-shell\"").is_err());
    assert!(parse("model_reasoning_effort=\"low\"").is_ok());
    assert!(
        RunArguments::parse(
            [
                "--agent",
                "claude",
                "-c",
                "service_tier=\"default\"",
                "--test",
                "true",
                "task"
            ]
            .map(str::to_owned)
            .to_vec()
        )
        .is_err()
    );
}
