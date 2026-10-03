use kah_observer::compaction::CompactorConfig;

#[test]
fn observer_defaults_to_the_permanent_model_floor() {
    let config = CompactorConfig::default();
    assert_eq!(config.model(), "gpt-6.1-sol");
    assert_eq!(config.reasoning(), "low");
}

#[test]
fn observer_accepts_exact_native_model_and_effort_choices() {
    let model = String::from("gpt-6.1-sol");
    let effort = String::from("high");
    let config = CompactorConfig::new(&model, Some(&effort)).unwrap();
    assert_eq!(config.model(), model);
    assert_eq!(config.reasoning(), effort);
    assert_eq!(
        CompactorConfig::new("luna", Some("low"))
            .unwrap()
            .reasoning(),
        "low"
    );
}

#[test]
fn invalid_model_and_effort_values_fail_before_launch() {
    for model in ["", "-option", "two words", "name\nother"] {
        assert!(CompactorConfig::new(model, None).is_err());
    }
    assert!(CompactorConfig::new("gpt-6.1-sol", Some("")).is_err());
}

#[test]
fn observer_rejects_unrecognized_models_and_efforts_below_the_floor() {
    for model in ["gpt-5.6-sol", "gpt-6-luna-preview", "unknown", "haiku"] {
        assert!(CompactorConfig::requested(model, None).is_err(), "{model}");
    }
    for effort in ["none", "minimal", "unknown", "-option"] {
        assert!(CompactorConfig::requested("gpt-6.1-sol", Some(effort)).is_err());
    }
}

#[test]
fn observer_preserves_known_higher_model_and_effort_choices() {
    for model in [
        "gpt-6-astra",
        "claude-sonnet-5-5",
        "claude-opus-5-5",
        "claude-fable-5-1",
    ] {
        let config = CompactorConfig::new(model, Some("high")).unwrap();
        assert_eq!(config.model(), model);
        assert_eq!(config.reasoning(), "high");
    }
}

#[test]
fn older_claude_bindings_upgrade_without_selecting_the_old_model() {
    for (model, floor) in [
        ("haiku", "claude-sonnet-5-5"),
        ("claude-sonnet-5", "claude-sonnet-5-5"),
        ("claude-opus-5", "claude-opus-5-5"),
    ] {
        assert!(CompactorConfig::requested(model, None).is_err());
        assert_eq!(CompactorConfig::new(model, None).unwrap().model(), floor);
    }
}

#[test]
fn claude_native_aliases_resolve_to_known_floor_or_higher_models() {
    for (alias, model) in [
        ("sonnet", "claude-sonnet-5-5"),
        ("opus", "claude-opus-5-5"),
        ("fable", "claude-fable-5-1"),
    ] {
        assert_eq!(
            CompactorConfig::requested(alias, Some("max"))
                .unwrap()
                .model(),
            model
        );
    }
    assert!(CompactorConfig::requested("opus", Some("ultra")).is_err());
}

#[test]
fn explicit_below_floor_models_fail_while_saved_bindings_upgrade() {
    for model in ["luna", "gpt-6-sol", "gpt-reserve"] {
        assert!(CompactorConfig::requested(model, None).is_err());
        assert_eq!(
            CompactorConfig::new(model, None).unwrap().model(),
            "gpt-6.1-sol"
        );
    }
    for effort in ["none", "minimal"] {
        assert_eq!(
            CompactorConfig::new("gpt-6.1-sol", Some(effort))
                .unwrap()
                .reasoning(),
            "low"
        );
    }
}
