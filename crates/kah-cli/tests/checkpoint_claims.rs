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

#[test]
fn a_claim_written_while_the_claims_job_holds_its_lock_is_not_stranded() {
    // The checkpoint-worker lost wakeup (5f, 2026-10-17) in the claims job: a
    // Stop writes its claim while the previous Jev screen still runs, so
    // hook::spawn starts nothing. An idle worker's last claim was never screened.
    let worker = format!("claims-wakeup-unit-{}", std::process::id());
    let root = kah_cli::observer_review::state::Store::directory(&worker).unwrap();
    let events = root.join("claim-events");
    kah_core::run_store::ensure_private_directory(&events).unwrap();
    kah_cli::unattended_report::save(&events, "first.json", &serde_json::json!({"turn":"1"}))
        .unwrap();
    let lock = root.join("job-__checkpoint-claims.lock");
    let open = || {
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock)
            .unwrap()
    };
    let lease = open();
    lease.try_lock().unwrap();
    let mut passes = 0;
    kah_cli::observer_review::claims::run_with(&worker, &lease, || {
        passes += 1;
        // The screen: every claim present is judged and removed.
        for entry in std::fs::read_dir(&events).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        if passes == 1 {
            let claim = serde_json::json!({"turn":"2","native_assistant_text":"Phase done."});
            kah_cli::unattended_report::save(&events, "second.json", &claim).unwrap();
            // hook::spawn would skip here: the running job holds the lock.
            assert!(open().try_lock().is_err());
        }
        Ok(())
    })
    .unwrap();
    assert!(
        !kah_cli::observer_review::job_lease::has_json(&events).unwrap(),
        "the last claim was stranded"
    );
    assert_eq!(passes, 2);
    std::fs::remove_dir_all(root.parent().unwrap()).unwrap();
}
