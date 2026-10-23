use kah_codex::delivery::Message;
use kah_core::intent_history::{KAH_DELEGATE_PREFIXES, SourceProvider, SourceRef, SourceRole};
use serde_json::json;

#[test]
fn a_send_sender_is_accepted_only_when_both_adapters_read_it_as_a_delegate() {
    // A custom sender such as `reviewer` was typed as `[reviewer] ...` over
    // Herdr, which carries no client id, so both adapters classified it User.
    for sender in ["reviewer", "kah_bot", "kah-my_bot", "kah-", "kah-user"] {
        assert!(
            kah_cli::send_cli::delegate_message("worker", sender, "r", "Context.").is_err(),
            "{sender}"
        );
    }
    for sender in ["kah-builder", "kah-ci-bot", "kah-reviewer2"] {
        let text = kah_cli::send_cli::delegate_message("worker", sender, "r", "Context.")
            .unwrap()
            .text;
        assert_eq!(
            kah_codex::authorship::with_client_id(&text, None),
            SourceRole::Delegate,
            "{text}"
        );
        let row = json!({"type":"user","uuid":"u","sessionId":"worker","cwd":"/fixture","message":{"role":"user","content":text}});
        let snapshot = kah_claude::transcript::parse(
            &(row.to_string() + "\n"),
            "worker",
            std::path::Path::new("/fixture"),
        )
        .unwrap();
        assert!(snapshot.users.is_none(), "{text}");
    }
}

#[test]
fn every_builtin_send_and_relay_prefix_uses_the_shared_delegate_vocabulary() {
    // kah's own envelope contract, not an inference from native user-role transport.
    let emitters = [
        ("kah-builder", include_str!("../src/send_cli.rs")),
        (
            "kah-observer",
            include_str!("../src/observer_review/gate_approval/producer/builder.rs"),
        ),
        ("kah-relay", include_str!("../src/relay_transport.rs")),
    ];
    let mut messages = vec![];
    for (sender, source) in emitters {
        assert!(
            source.contains(&format!("\"{sender}\"")),
            "update the emitter coverage when its sender changes"
        );
        messages.push(
            Message::new("worker", sender, "r", "Context, not user authority.")
                .unwrap()
                .text,
        );
    }
    messages.push(
        Message::new("worker", "kah-reviewer", "r", "Delegate context.")
            .unwrap()
            .text,
    );
    messages.push(
        kah_observer::relayed_user::Message::new("kah-reviewer", "r", "A relayed decision.")
            .unwrap()
            .wire(),
    );
    messages.push(verified_relay());
    for text in messages {
        let prefix = text.split_whitespace().next().unwrap();
        assert!(
            KAH_DELEGATE_PREFIXES.contains(&prefix),
            "unrecognized kah sender prefix: {prefix}"
        );
        assert_eq!(
            kah_codex::authorship::classify(&text, false),
            SourceRole::Delegate
        );
        let row = json!({"type":"user","uuid":"u","sessionId":"worker","cwd":"/fixture","message":{"role":"user","content":text}});
        let snapshot = kah_claude::transcript::parse(
            &(row.to_string() + "\n"),
            "worker",
            std::path::Path::new("/fixture"),
        )
        .unwrap();
        assert!(
            snapshot.users.is_none(),
            "unverified envelope cannot become user authority"
        );
    }
}
fn verified_relay() -> String {
    kah_cli::relay::Receipt {
        id: "r".into(),
        sender: "kah-reviewer".into(),
        receiver: "worker".into(),
        quote: kah_observer::inherited::Quote {
            receipt: "r".into(),
            source: SourceRef::new(
                SourceProvider::Codex,
                "source",
                "u",
                SourceRole::User,
                "sha256:example",
            ),
            text: "Keep the user's constraint.".into(),
            dispatch_item: None,
        },
        paraphrase: "Delegate paraphrase.".into(),
        supported: false,
        judgment: None,
        judgment_error: None,
    }
    .message()
}
