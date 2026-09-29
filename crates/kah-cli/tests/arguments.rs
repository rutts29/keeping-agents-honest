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
    assert_eq!(
        args.native,
        vec!["--model", "chosen", "-c", "model_reasoning_effort=\"high\""]
    );
    assert!(parse(&[]).native.is_empty());
}

#[test]
fn restricted_native_arguments_allow_resume_and_models_but_reject_policy_bypasses() {
    for args in [
        vec!["--", "resume", "--all"],
        vec!["--", "-m", "chosen"],
        vec!["--", "-c", "model_reasoning_effort=high"],
    ] {
        assert!(parse(&args).validate_native(Domain::Research).is_ok());
    }
    for args in [
        vec!["--", "--search"],
        vec!["--", "-c", "features.apps=true"],
        vec!["--", "--remote", "unix:///socket"],
        vec!["--", "exec", "task"],
        vec!["--", "--yolo"],
        vec!["--", "--config=web_search=live"],
    ] {
        assert!(
            parse(&args).validate_native(Domain::Untrusted).is_err(),
            "{args:?}"
        );
    }
}
