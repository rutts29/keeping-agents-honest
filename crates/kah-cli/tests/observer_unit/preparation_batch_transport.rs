use super::*;
#[test]
fn a_rejected_selection_reopens_only_that_decision_and_keeps_other_manifests() {
    use crate::observer_review::state::State;
    let worker = format!("batch-selective-retry-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("workspace");
    std::fs::create_dir_all(workspace.join("outbox")).unwrap();
    let state:State=serde_json::from_value(serde_json::json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let jobs = two_jobs(&worker);
    stage(&worker, "review-retry-owner", &jobs).unwrap();
    let manifest = all_selections(&jobs).remove(1);
    let name = format!("selection-{}.json", jobs[1].decision);
    source_store::save(
        &store.directory,
        &name,
        &serde_json::to_value(&manifest).unwrap(),
    )
    .unwrap();
    let before = std::fs::read(store.directory.join(&name)).unwrap();
    super::super::reopen(&worker, &jobs[0].decision).unwrap();
    let current: serde_json::Value = state::read(&store.directory.join("preparation-current.json"))
        .unwrap()
        .unwrap();
    let batch = load(&worker, current["decision"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(batch.decisions.len(), 1);
    assert_eq!(batch.decisions[0].decision, jobs[0].decision);
    assert_eq!(std::fs::read(store.directory.join(&name)).unwrap(), before);
    assert!(
        store
            .directory
            .join(format!("preparation-{}.json", jobs[1].decision))
            .exists()
    );
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn staging_round_trips_whole_jobs_and_digests_without_partial_acceptance() {
    let worker = format!("batch-store-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let original = serde_json::json!({"intent":{"goal":"Verbatim full user goal","items":[["user-one","Keep all confidential notes local."]]},"event":{"actions":[]}});
    let jobs = vec![Job {
        worker: worker.clone(),
        decision: "review-batch-one".into(),
        state: original.clone(),
        questions: Default::default(),
        items: kah_observer::context_selection::items(
            &original,
            &std::collections::BTreeSet::from(["user-one".into()]),
        ),
        reason: "Original context rejection".into(),
    }];
    let batch = stage(&worker, "review-owner", &jobs).unwrap();
    assert_eq!(load(&worker, &batch.id).unwrap().unwrap().id, batch.id);
    let restored = super::jobs(&worker, &batch).unwrap();
    assert_eq!(restored[0].state, original);
    assert!(
        restored[0]
            .items
            .iter()
            .any(|item| item.active_user && item.pointer == "/intent/items/0")
    );
    let mut changed = jobs[0].clone();
    changed.reason = "Changed job".into();
    source_store::save(
        &store.directory,
        &format!("preparation-batch-job-{}.json", batch.decisions[0].digest),
        &serde_json::to_value(changed).unwrap(),
    )
    .unwrap();
    assert!(
        super::jobs(&worker, &batch)
            .err()
            .unwrap()
            .to_string()
            .contains("PREPARATION_BATCH_CHANGED")
    );
    let directory = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn incomplete_batch_publishes_no_ack_then_whole_batch_can_be_submitted_and_accepted() {
    use crate::observer_review::state::{Pending, Purpose, State};
    let worker = format!("batch-submit-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let workspace = store.directory.join("observer");
    let input = workspace.join("inputs/review-batch");
    let outbox = workspace.join("outbox");
    for directory in [&input, &outbox] {
        kah_core::run_store::ensure_private_directory(directory).unwrap();
    }
    let state:State=serde_json::from_value(serde_json::json!({"worker":{"provider":"codex","session":worker,"project":workspace,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":workspace,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":Pending{id:"review-batch".into(),purpose:Purpose::Preparation,turn:"original".into(),head:None,created_ms:0,native_tokens_before:Some(0)},"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let jobs = two_jobs(&worker);
    let batch = stage(&worker, "review-batch", &jobs).unwrap();
    let index = input.join("batch.json");
    source_store::save(&input, "batch.json", &serde_json::to_value(&batch).unwrap()).unwrap();
    for job in &jobs {
        source_store::save(
            &input,
            &format!("preparation-{}.json", job.decision),
            &serde_json::to_value(job).unwrap(),
        )
        .unwrap();
    }
    let choices = all_selections(&jobs);
    let output = outbox.join("choices.json");
    let args = vec![
        "observe".into(),
        "select-batch".into(),
        "--request".into(),
        index.to_string_lossy().into_owned(),
        "--json".into(),
        output.to_string_lossy().into_owned(),
    ];
    write_submission(&output, &batch.id, vec![choices[0].clone()]);

    assert!(submit(&args).is_err());
    assert!(!outbox.join(format!("selected-{}.json", batch.id)).exists());
    assert!(!outbox.join("selected-review-original-0.json").exists());
    write_submission(&output, &batch.id, choices);

    submit(&args).unwrap();
    assert!(super::super::accept(&worker, &batch).unwrap());
    for job in &jobs {
        assert!(
            store
                .directory
                .join(format!("selection-{}.json", job.decision))
                .exists()
        );
    }
    let directory = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}

fn two_jobs(worker: &str) -> Vec<Job> {
    (0..2)
        .map(|i| {
            let state = serde_json::json!({"intent":{"goal":format!("Whole original goal {i}")}});
            Job {
                decision: format!("review-original-{i}"),
                worker: worker.into(),
                items: kah_observer::context_selection::items(&state, &Default::default()),
                state,
                questions: Default::default(),
                reason: "Original rejection".into(),
            }
        })
        .collect::<Vec<_>>()
}

fn all_selections(jobs: &[Job]) -> Vec<kah_observer::context_selection::Manifest> {
    use kah_observer::context_selection::{Manifest, Selection};
    jobs.iter()
        .map(|job| Manifest {
            decision: job.decision.clone(),
            selected: job
                .items
                .iter()
                .map(|item| Selection {
                    id: item.id.clone(),
                    digest: item.digest.clone(),
                    reason: "Relevant whole item".into(),
                    superseded_by: None,
                })
                .collect(),
            excluded: vec![],
        })
        .collect::<Vec<_>>()
}

fn write_submission(
    path: &Path,
    batch: &str,
    selections: Vec<kah_observer::context_selection::Manifest>,
) {
    std::fs::write(
        path,
        serde_json::to_vec(&Submission {
            batch: batch.into(),
            selections,
        })
        .unwrap(),
    )
    .unwrap();
}
