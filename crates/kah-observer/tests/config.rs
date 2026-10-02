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
