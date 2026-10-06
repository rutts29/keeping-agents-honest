use kah_observer::judge::{Answer, Judgment};
use std::collections::BTreeMap;
fn judgment(noul: f64) -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers: BTreeMap::from([("phase_claim".into(), Answer::Noul { noul })]),
        metrics: Default::default(),
        fallback_reason: None,
    }
}
#[test]
fn the_real_claude_partial_phase_claim_cannot_be_silently_ignored() {
    assert!(kah_cli::observer_review::claims::needs_review(&judgment(
        0.76
    )));
    assert!(kah_cli::observer_review::claims::needs_review(&judgment(
        0.99
    )));
    assert!(!kah_cli::observer_review::claims::needs_review(&judgment(
        0.02
    )));
}

#[test]
fn a_verified_non_claim_is_neutral_and_doubt_stays_pending() {
    use kah_cli::observer_review::checkpoint_claim;
    let mut j = judgment(0.02);
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.02 });
    let neutral = checkpoint_claim::assessment(&j).unwrap().unwrap();
    assert_eq!(neutral["state"], "checkpoint_not_claimed");
    assert!(neutral["passed"].is_null());
    j.backend = "fixture".into();
    assert!(checkpoint_claim::assessment(&j).is_err());
    j.backend = "jev".into();
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.76 });
    assert!(
        checkpoint_claim::assessment(&j)
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_PHASE_CLAIM_UNCERTAIN")
    );
    j.answers
        .insert("claim_made".into(), Answer::Noul { noul: 0.99 });
    assert!(checkpoint_claim::assessment(&j).unwrap().is_none());
}

#[test]
fn doubtful_claim_requires_thinker_selected_native_assistant_evidence() {
    // Native source: openai/codex rust-v0.160.0, protocol/src/items.rs
    // TurnItem::AgentMessage. Existing host rendering supplies provenance;
    // marker-looking quote text is never the source of the record type.
    use kah_cli::observer_review::{
        checkpoint_claim,
        state::{Purpose, Request},
    };
    use serde_json::json;
    let request = Request {
        id: "review-claim".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: json!({}),
        trigger: json!({"events":[{"kind":"native_phase_claim","uncertain":true}]}),
        documents: vec![],
        created_ms: 0,
    };
    let unrelated = json!({"topic":"intent","citations":[{"native_records":[{"message_role":"assistant","native_transport_only":true}]}]});
    assert!(checkpoint_claim::require_evidence(&request, &[unrelated]).is_err());
    let tool = json!({"topic":"claim","citations":[{"native_records":[{"message_role":"tool","native_transport_only":true}]}]});
    assert!(checkpoint_claim::require_evidence(&request, &[tool]).is_err());
    let claim = json!({"topic":"claim","citations":[{"native_records":[{"message_role":"assistant","native_transport_only":true}]}]});
    checkpoint_claim::require_evidence(&request, &[claim]).unwrap();
}
