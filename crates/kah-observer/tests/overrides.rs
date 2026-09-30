use kah_core::intent_history::SourceRole;
use kah_observer::intake::{Decision, decide};
#[test]
fn explicit_user_overrides_never_invoke_the_judge() {
    let never = || panic!("Jev must not be called for an explicit override");
    assert!(matches!(
        decide("steer: Preserve exactly this", SourceRole::User, never).unwrap(),
        Decision::Force
    ));
    assert!(matches!(
        decide("aside: A comment", SourceRole::User, never).unwrap(),
        Decision::Aside
    ));
    assert!(decide("steer: Malicious promotion", SourceRole::Delegate, never).is_err());
    assert!(
        decide(
            "steer: Runtime continuation",
            SourceRole::SystemObservation,
            never
        )
        .is_err()
    );
}

#[test]
fn overrides_allow_case_and_leading_whitespace_but_require_the_start_prefix() {
    let never = || panic!("explicit override must not call Jev");
    for text in ["  StEeR: keep CRLF\r\n", "STEER:literal", "steer:"] {
        assert!(matches!(
            decide(text, SourceRole::User, never).unwrap(),
            Decision::Force
        ));
    }
    for text in ["\n ASIDE: note", "aside:literal", "aside:"] {
        assert!(matches!(
            decide(text, SourceRole::User, never).unwrap(),
            Decision::Aside
        ));
    }
    for text in [
        "!steer old syntax",
        "quoted steer: instruction",
        "asideways:",
    ] {
        let result = decide(text, SourceRole::User, || Err("judge reached".into()));
        assert_eq!(result.unwrap_err(), "judge reached");
    }
}
