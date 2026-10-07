use super::*;
#[test]
fn delivery_noise_cannot_undo_ack_but_host_errors_are_not_retried_as_success() {
    for error in [
        "REVIEW_REPORT_MISSING stage=observer",
        "REVIEW_OBSERVER_STALLED stage=liveness",
        "REVIEW_BILLING_ABORT",
        "REVIEW_SUBMISSION_UNCONFIRMED stage=delivery",
    ] {
        assert!(!correction_required(error));
    }
    for error in [
        "JUDGE_CHOICE stage=judge",
        "JEV_HTTP_STATUS stage=judge",
        "REVIEW_PHASE_CLAIM_UNCERTAIN stage=checkpoint",
        "REVIEW_SOURCE_DIGEST stage=source_store",
    ] {
        assert!(
            correction_required(error),
            "host failure must remain visible, not repeated every sample: {error}"
        );
    }
}

#[test]
fn selection_ack_and_preparation_consume_belong_to_host_without_an_evidence_ack() {
    use crate::observer_review::state::{Pending, Purpose};
    use serde_json::json;
    let worker = format!("selection-handoff-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("observer");
    std::fs::create_dir_all(workspace.join("outbox")).unwrap();
    let mut state:State = serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let decision = "review-selection";
    state.pending = Some(Pending {
        id: decision.into(),
        purpose: Purpose::Preparation,
        turn: "t".into(),
        head: None,
        created_ms: 0,
        native_tokens_before: None,
    });
    store.save(&state).unwrap();
    let items = selection_job(&store, &worker, decision);
    assert!(!awaiting_host(&state).unwrap());
    submit_selection(&workspace, decision, &items);
    assert!(
        awaiting_host(&state).unwrap(),
        "context_selection_submitted hands ownership to host"
    );
    assert!(
        store
            .directory
            .join(format!("selection-{decision}.json"))
            .exists()
    );
    // Host preparation remains pending while its Jev decision runs. It never
    // requires an evidence_submitted marker to suppress a false redelivery.
    assert!(preparation::pending(&worker, decision).unwrap());
    assert!(awaiting_host(&state).unwrap());
    assert!(
        !workspace
            .join("outbox")
            .join(format!("submitted-{decision}.json"))
            .exists()
    );
    let directory = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

fn selection_job(
    store: &Store,
    worker: &str,
    decision: &str,
) -> Vec<kah_observer::context_selection::Item> {
    use kah_observer::context_selection;
    use serde_json::json;
    let original = json!({"intent":{"goal":"Whole user goal."},"event":{"actions":[]}});
    let items = context_selection::items(&original, &std::collections::BTreeSet::new());
    let job = preparation::Job {
        decision: decision.into(),
        worker: worker.into(),
        state: original,
        questions: Default::default(),
        items: items.clone(),
        reason: "provider budget".into(),
    };
    crate::observer_review::source_store::save(
        &store.directory,
        &format!("preparation-{decision}.json"),
        &serde_json::to_value(job).unwrap(),
    )
    .unwrap();
    items
}

fn submit_selection(
    workspace: &std::path::Path,
    decision: &str,
    items: &[kah_observer::context_selection::Item],
) {
    use kah_observer::context_selection::{Manifest, Selection};
    let manifest = Manifest {
        decision: decision.into(),
        selected: items
            .iter()
            .map(|i| Selection {
                id: i.id.clone(),
                digest: i.digest.clone(),
                reason: "Whole relevant item".into(),
                superseded_by: None,
            })
            .collect(),
        excluded: vec![],
    };
    std::fs::write(
        workspace
            .join("outbox")
            .join(format!("selected-{decision}.json")),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
}
