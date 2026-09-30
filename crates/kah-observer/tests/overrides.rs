use kah_core::intent_history::SourceRole;
use kah_observer::intake::{Decision, decide};
#[test]
fn explicit_user_overrides_never_invoke_the_judge() {
    let never = || panic!("Jev must not be called for an explicit override");
    assert!(matches!(
        decide("!steer Preserve exactly this", SourceRole::User, never).unwrap(),
        Decision::Force
    ));
    assert!(matches!(
        decide("!aside A comment", SourceRole::User, never).unwrap(),
        Decision::Aside
    ));
    assert!(decide("!steer Malicious promotion", SourceRole::Delegate, never).is_err());
    assert!(
        decide(
            "!steer Runtime continuation",
            SourceRole::SystemObservation,
            never
        )
        .is_err()
    );
}
