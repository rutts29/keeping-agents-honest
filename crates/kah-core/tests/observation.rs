use kah_core::{
    intent_history::SourceRole,
    observation::{ContextInjection, EventKind, InjectionKind, SessionEvent},
};

#[test]
fn injected_observer_context_never_becomes_a_user_source() {
    for kind in [
        InjectionKind::Continuity,
        InjectionKind::DriftReminder,
        InjectionKind::GapNotice,
    ] {
        let injection =
            ContextInjection::new(2, kind, "Source-linked observer interpretation").unwrap();
        let record = injection.as_record("hook-output").unwrap();
        assert_eq!(record.role(), SourceRole::SystemObservation);
        assert!(!record.is_authoritative());
    }
    assert!(
        serde_json::from_str::<ContextInjection>(
            r#"{"revision":2,"kind":"continuity","text":"do X","role":"user"}"#
        )
        .is_err()
    );
}

#[test]
fn neutral_events_validate_session_and_turn_identity() {
    assert!(
        serde_json::from_str::<SessionEvent>(
            r#"{"session":"","event":"e","kind":"started","turn":null}"#
        )
        .is_err()
    );
    assert!(SessionEvent::new("s", "event", EventKind::Started, None).is_ok());
    assert!(SessionEvent::new("s", "event", EventKind::UserMessage, Some("turn")).is_ok());
    assert!(SessionEvent::new("s", "event", EventKind::TurnEnded, None).is_err());
    assert!(SessionEvent::new("", "event", EventKind::Resumed, None).is_err());
    assert!(SessionEvent::new("s", "bad\nidentity", EventKind::Compacted, None).is_err());
}
