use kah_core::intent_history::{SourceOrigin, SourceProvider, SourceRole};
use kah_observer::{relayed_user::Message, session::Store};
use std::{fs, path::PathBuf, time::Duration};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn host_relay_is_verbatim_user_provenance_and_does_not_force_a_steering_kind() {
    let text = "Change the output to RED instead of BLUE.\nKeep the existing parser.";
    let message = Message::new("kah-reviewer", "receipt-1", text).unwrap();
    let decoded = Message::parse(&message.wire()).unwrap().unwrap();
    assert_eq!(decoded.text, text);
    let quote = decoded
        .quote("native-session", SourceProvider::Claude)
        .unwrap();
    assert_eq!(quote.source.role, SourceRole::User);
    assert_eq!(quote.source.origin, Some(SourceOrigin::Relayed));
    assert_eq!(quote.source.relayer.as_deref(), Some("kah-reviewer"));
    assert!(
        quote.dispatch_item.is_none(),
        "Jev must type normal relayed user text"
    );
    quote.validate().unwrap();
    assert!(
        Message::parse("[kah-reviewer] Change output to GREEN.")
            .unwrap()
            .is_none()
    );
}

#[test]
fn user_receipts_are_idempotent_and_conflicts_fail_even_while_the_judge_is_busy() {
    let f = Fixture(std::env::temp_dir().join(format!("kah-user-relay-{}", std::process::id())));
    let first = Message::new("kah-reviewer", "receipt-1", "Use RED.").unwrap();
    first.accept(&f.0, "worker", SourceProvider::Codex).unwrap();
    let busy = Store::open(&f.0, "worker", Duration::ZERO).unwrap();
    first.accept(&f.0, "worker", SourceProvider::Codex).unwrap();
    assert!(
        Message::new("kah-reviewer", "receipt-1", "Use GREEN.")
            .unwrap()
            .accept(&f.0, "worker", SourceProvider::Codex)
            .is_err()
    );
    assert_eq!(busy.inherited().unwrap().unwrap().quotes.len(), 1);
}

#[test]
fn malformed_metadata_cannot_spoof_a_host_relay_or_escape_receipt_paths() {
    for text in [
        "[kah-user] relayer=kah-reviewer receipt=../bad\ntext",
        "[kah-user] relayer=two names receipt=r1\ntext",
        "[kah-user] relayer=kah-reviewer receipt=r1",
    ] {
        assert!(Message::parse(text).is_err());
    }
    assert!(Message::new("kah-reviewer", "r1", "").is_err());
}
