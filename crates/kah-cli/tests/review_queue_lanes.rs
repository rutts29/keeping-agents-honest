use kah_cli::observer_review::{
    queue::{
        Wake,
        lanes::{self, Lane, Stamp},
    },
    state::Purpose,
};
use serde_json::json;
use std::{collections::BTreeMap, path::PathBuf};
fn jobs() -> Vec<(PathBuf, Wake)> {
    [Purpose::Checkpoint, Purpose::Drift, Purpose::Checkpoint]
        .into_iter()
        .enumerate()
        .map(|(i, purpose)| {
            (
                PathBuf::from(format!("{i}.json")),
                Wake {
                    purpose,
                    turn: i.to_string(),
                    trigger: json!({}),
                },
            )
        })
        .collect()
}
#[test]
fn aged_intent_head_precedes_gates_and_rotates_after_gate_without_a_deadline() {
    let mut entries = jobs();
    let stamps = BTreeMap::from([
        (
            "0.json".into(),
            Stamp {
                at_ms: 900_000,
                lane: Lane::Gate,
            },
        ),
        (
            "1.json".into(),
            Stamp {
                at_ms: 0,
                lane: Lane::Intent,
            },
        ),
        (
            "2.json".into(),
            Stamp {
                at_ms: 950_000,
                lane: Lane::Gate,
            },
        ),
    ]);
    lanes::sort(&mut entries, &stamps, None, 1_000_000).unwrap();
    assert_eq!(entries[0].1.purpose, Purpose::Drift);
    let mut entries = jobs();
    let fresh = stamps
        .iter()
        .map(|(k, s)| {
            (
                k.clone(),
                Stamp {
                    at_ms: 1_000_000,
                    ..s.clone()
                },
            )
        })
        .collect();
    lanes::sort(&mut entries, &fresh, Some(Lane::Gate), 1_000_001).unwrap();
    assert_eq!(
        entries[0].1.purpose,
        Purpose::Drift,
        "one gate cannot monopolize another waiting lane"
    );
}
#[test]
fn typed_commit_lane_is_distinct_and_gate_only_queues_keep_existing_final_review_priority() {
    let mut entries = jobs();
    entries[1].1.purpose = Purpose::Checkpoint;
    entries[1].1.trigger = json!({"review_class":"commit"});
    assert_eq!(Lane::of(&entries[1].1), Lane::Commit);
    entries[2].1.trigger = json!({"verification_followup":{"stage":"checks_complete"}});
    let stamps = entries
        .iter()
        .map(|(p, w)| {
            (
                p.to_str().unwrap().into(),
                Stamp {
                    at_ms: 1000,
                    lane: Lane::of(w),
                },
            )
        })
        .collect();
    lanes::sort(&mut entries, &stamps, None, 1001).unwrap();
    assert_eq!(entries[0].0, PathBuf::from("2.json"));
}
#[test]
fn even_an_older_aged_gate_cannot_take_two_turns_when_another_lane_waits() {
    let mut entries = jobs();
    let stamps = BTreeMap::from([
        (
            "0.json".into(),
            Stamp {
                at_ms: 0,
                lane: Lane::Gate,
            },
        ),
        (
            "1.json".into(),
            Stamp {
                at_ms: 1000,
                lane: Lane::Intent,
            },
        ),
        (
            "2.json".into(),
            Stamp {
                at_ms: 2000,
                lane: Lane::Gate,
            },
        ),
    ]);
    lanes::sort(&mut entries, &stamps, Some(Lane::Gate), 1_000_000).unwrap();
    assert_eq!(entries[0].1.purpose, Purpose::Drift);
}
