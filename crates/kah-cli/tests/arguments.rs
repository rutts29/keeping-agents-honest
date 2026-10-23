use kah_cli::Arguments;
use kah_sandbox::Domain;

fn parse(args: &[&str]) -> Arguments {
    Arguments::parse(args.iter().map(|s| s.to_string()).collect()).unwrap()
}

#[test]
fn domain_and_native_model_choices_are_not_pinned() {
    let args = parse(&[
        "--online", "research", "demo", "--model", "chosen", "--effort", "high",
    ]);
    assert_eq!(args.project, Some((Domain::Research, "demo".into())));
    assert!(args.options.online);
    assert_eq!(args.native, vec!["--model", "chosen"]);
    assert_eq!(args.effort.as_deref(), Some("high"));
    assert!(parse(&[]).effort.is_none());
    assert!(parse(&[]).native.is_empty());
}

#[test]
fn claude_selection_preserves_native_choices_and_resume_arguments() {
    let args = parse(&[
        "--agent", "claude", "--model", "chosen", "--effort", "high", "--", "--resume", "id",
    ]);
    assert_eq!(args.agent.as_deref(), Some("claude"));
    assert_eq!(args.native, ["--model", "chosen", "--resume", "id"]);
    assert_eq!(args.effort.as_deref(), Some("high"));
    assert!(parse(&[]).agent.is_none());
}

#[test]
fn observer_choices_are_separate_from_the_main_model_and_default_on() {
    assert!(!parse(&[]).no_observer);
    let args = parse(&[
        "--observer-model",
        "gpt-6.1-sol",
        "--observer-effort",
        "low",
        "--model",
        "chosen",
    ]);
    assert_eq!(args.observer_model.as_deref(), Some("gpt-6.1-sol"));
    assert_eq!(args.observer_effort.as_deref(), Some("low"));
    assert_eq!(args.native, vec!["--model", "chosen"]);
    assert!(parse(&["--no-observer"]).no_observer);
}

#[test]
fn restricted_native_arguments_allow_resume_and_models_but_reject_policy_bypasses() {
    for args in [
        vec!["--", "resume", "--all"],
        vec!["--", "-m", "chosen"],
        vec!["--", "-c", "model_reasoning_effort=high"],
        vec!["--", "--search"],
    ] {
        assert!(parse(&args).validate_native(Domain::Research).is_ok());
    }
    for args in [
        vec!["--", "-c", "features.apps=true"],
        vec!["--", "--remote", "unix:///socket"],
        vec!["--", "exec", "task"],
        vec!["--", "agents"],
        vec!["--", "--yolo"],
        vec!["--", "--config=web_search=live"],
    ] {
        assert!(
            parse(&args).validate_native(Domain::Untrusted).is_err(),
            "{args:?}"
        );
    }
}

#[test]
fn an_attached_model_value_configures_the_app_server_like_a_separate_one() {
    for args in [
        vec!["--model", "chosen"],
        vec!["--model=chosen"],
        vec!["--", "--model=chosen"],
        vec!["mine", "demo", "--model=chosen", "resume"],
    ] {
        assert_eq!(parse(&args).native_model(), Some("chosen"), "{args:?}");
    }
    let args = parse(&["--model=chosen"]);
    assert_eq!(args.native, ["--model", "chosen"]);
    for domain in [Domain::Research, Domain::Untrusted] {
        assert!(args.validate_native(domain).is_ok());
        assert!(
            parse(&["--", "--model=chosen"])
                .validate_native(domain)
                .is_ok()
        );
    }
}
