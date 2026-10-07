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

#[test]
fn cross_packet_snapshot_preserves_order_and_later_arrivals_stay_outside() {
    use kah_cli::observer_review::{
        preparation::batch::packets::job_for_packet,
        queue::{Wake, batch},
        state::{Purpose, Request},
    };
    let originals = jobs();
    let entries=originals.iter().enumerate().map(|(index,job)| (std::path::PathBuf::from(format!("original-{index}.json")),Wake{purpose:Purpose::Preparation,turn:format!("turn-{index}"),trigger:json!({"preparation":{"state":job.state,"questions":job.questions,"purpose":"drift","reason":job.reason}})})).collect::<Vec<_>>();
    let (wake, paths) = batch(&entries).unwrap();
    assert_eq!(paths.len(), 22);
    assert_eq!(
        wake.trigger["preparation_packets"]
            .as_array()
            .unwrap()
            .len(),
        22
    );
    let request = Request {
        id: "review-parent".into(),
        worker: "worker".into(),
        purpose: Purpose::Preparation,
        turn: "current-turn".into(),
        head: None,
        intent: json!({"goal":"Today's different goal","items":[]}),
        trigger: wake.trigger,
        documents: vec![],
        created_ms: 100,
    };
    let packet = &request.trigger["preparation_packets"][4];
    let job = job_for_packet(&request, 4, packet).unwrap();
    assert_eq!(job.state, originals[4].state);
    assert!(
        job.items
            .iter()
            .any(|i| i.pointer == "/intent/items/0" && i.active_user)
    );
    let mut later = entries;
    later.push((
        "later.json".into(),
        Wake {
            purpose: Purpose::Preparation,
            turn: "later".into(),
            trigger: json!({"different":true}),
        },
    ));
    assert_eq!(
        request.trigger["preparation_packets"]
            .as_array()
            .unwrap()
            .len(),
        22
    );
    assert!(!paths.contains(&std::path::PathBuf::from("later.json")));
}

#[test]
fn packet_batches_keep_native_snapshots_and_other_purposes_in_their_lanes() {
    use kah_cli::observer_review::{
        queue::{Wake, batch, lanes::Lane},
        state::Purpose,
    };
    let wake = |purpose: &str| Wake {
        purpose: Purpose::Preparation,
        turn: purpose.into(),
        trigger: json!({"preparation":{"purpose":purpose,"state":{"intent":{"goal":"Whole"}},"questions":{}}}),
    };
    let drift = wake("drift");
    let gate = wake("gate");
    let entries = vec![
        ("drift.json".into(), drift),
        ("gate.json".into(), gate.clone()),
    ];
    let (group, paths) = batch(&entries).unwrap();
    assert_eq!(paths, vec![std::path::PathBuf::from("drift.json")]);
    assert_eq!(Lane::of(&group), Lane::Intent);
    let (group, paths) = batch(&[entries[1].clone()]).unwrap();
    assert_eq!(Lane::of(&group), Lane::Gate);
    assert_eq!(paths.len(), 1);
    let mut snapshot = gate;
    snapshot.trigger["captured_snapshot"] = json!({"id":"exact-snapshot"});
    let (group, paths) = batch(&[
        ("snapshot.json".into(), snapshot.clone()),
        entries[0].clone(),
    ])
    .unwrap();
    assert_eq!(paths, vec![std::path::PathBuf::from("snapshot.json")]);
    assert_eq!(group.trigger, snapshot.trigger);
}
