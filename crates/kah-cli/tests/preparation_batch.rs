use kah_cli::observer_review::preparation::{
    Job,
    batch::{Batch, validate},
};
use kah_observer::{
    context_selection::{self, Manifest, Selection},
    judge::Questions,
};
use serde_json::json;
use std::collections::BTreeSet;
fn jobs() -> Vec<Job> {
    (0..22).map(|index| {
        let state=json!({"intent":{"goal":format!("Original goal {index}"),"items":[[format!("user-{index}"),format!("Verbatim authority {index}")]]},"event":{"actions":[{"id":format!("action-{index}"),"text":"Whole action"}]}});
        Job{decision:format!("review-packet-{index}"),worker:"worker".into(),items:context_selection::items(&state,&BTreeSet::from([format!("user-{index}")])),state,questions:Questions::new(),reason:"Original provider rejection".into()}
    }).collect()
}
fn selections(jobs: &[Job]) -> Vec<Manifest> {
    jobs.iter()
        .map(|job| Manifest {
            decision: job.decision.clone(),
            selected: job
                .items
                .iter()
                .map(|item| Selection {
                    id: item.id.clone(),
                    digest: item.digest.clone(),
                    reason: "Whole relevant item".into(),
                    superseded_by: None,
                })
                .collect(),
            excluded: vec![],
        })
        .collect()
}
#[test]
fn all_22_original_decisions_require_individual_complete_checked_manifests() {
    let jobs = jobs();
    let batch = Batch::new("review-parent", "worker", &jobs).unwrap();
    assert_eq!(batch.decisions.len(), 22);
    let mut choices = selections(&jobs);
    assert_eq!(validate(&batch, &jobs, &choices).unwrap().len(), 22);
    choices.pop();
    assert!(
        validate(&batch, &jobs, &choices)
            .unwrap_err()
            .to_string()
            .contains("review-packet-21")
    );
    choices.push(choices[0].clone());
    assert!(validate(&batch, &jobs, &choices).is_err());
}
#[test]
fn every_original_authority_is_protected_and_batch_identity_is_ordered() {
    let jobs = jobs();
    let batch = Batch::new("review-parent", "worker", &jobs).unwrap();
    let mut choices = selections(&jobs);
    let selected = choices[4]
        .selected
        .iter()
        .position(|s| s.id == "/intent/items/0")
        .unwrap();
    let omitted = choices[4].selected.remove(selected);
    choices[4].excluded.push(omitted);
    assert!(
        validate(&batch, &jobs, &choices)
            .unwrap_err()
            .to_string()
            .contains("CONTEXT_USER_AUTHORITY")
    );
    let mut reversed = jobs.clone();
    reversed.reverse();
    assert_ne!(
        batch.id,
        Batch::new("review-parent", "worker", &reversed).unwrap().id
    );
    reversed[0].worker = "other-worker".into();
    assert!(Batch::new("review-parent", "worker", &reversed).is_err());
}
