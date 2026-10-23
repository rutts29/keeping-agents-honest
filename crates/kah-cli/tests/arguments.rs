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

#[test]
fn the_native_model_ignores_prompt_text_and_other_option_values() {
    // The native `--` ends options; values of other options are not models.
    for (args, expected) in [
        (
            vec!["--model", "good", "--", "--", "--model=bad"],
            Some("good"),
        ),
        (vec!["--", "--image", "--model=bad"], None),
        (
            vec!["--", "-c", "--model=bad", "--model=good"],
            Some("good"),
        ),
        (
            vec!["--", "-i", "shot.png", "--model", "good"],
            Some("good"),
        ),
        (vec!["--", "--profile", "--model=bad"], None),
    ] {
        assert_eq!(parse(&args).native_model(), expected, "{args:?}");
    }
}

#[test]
fn every_attached_model_spelling_is_the_native_model() {
    for args in [
        vec!["-mchosen"],
        vec!["-m=chosen"],
        vec!["--", "-mchosen"],
        vec!["--", "-m=chosen"],
        vec!["mine", "demo", "-mchosen", "resume"],
        vec!["-m", "other", "-m=chosen"],
    ] {
        assert_eq!(parse(&args).native_model(), Some("chosen"), "{args:?}");
    }
    assert_eq!(parse(&["-mchosen"]).native, ["--model", "chosen"]);
    assert_eq!(parse(&["-m=chosen"]).native, ["--model", "chosen"]);
    for domain in [Domain::Research, Domain::Untrusted] {
        for args in [vec!["-mchosen"], vec!["--", "-m=chosen"]] {
            assert!(parse(&args).validate_native(domain).is_ok(), "{args:?}");
        }
    }
}

#[test]
fn an_attached_model_is_not_one_after_the_native_terminator_or_inside_a_value() {
    for args in [
        vec!["--", "--", "-mbad"],
        vec!["--", "-p", "-mbad"],
        vec!["--", "-i", "-m=bad"],
    ] {
        assert_eq!(parse(&args).native_model(), None, "{args:?}");
    }
}

#[test]
fn a_value_option_may_carry_a_word_that_is_also_a_subcommand_in_mine() {
    // `--profile exec` names a profile; it is not the forbidden exec command.
    let args = parse(&["--", "--profile", "exec"]);
    assert!(args.validate_native(Domain::Mine).is_ok());
    for domain in [Domain::Research, Domain::Untrusted] {
        assert!(args.validate_native(domain).is_err());
    }
    assert!(
        parse(&["--", "exec"])
            .validate_native(Domain::Mine)
            .is_err()
    );
}
