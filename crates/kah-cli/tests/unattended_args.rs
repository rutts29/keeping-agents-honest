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
        vec!["--online", "--test", "true", "task"],
    ] {
        assert!(RunArguments::parse(args.into_iter().map(str::to_owned).collect()).is_err());
    }
}
