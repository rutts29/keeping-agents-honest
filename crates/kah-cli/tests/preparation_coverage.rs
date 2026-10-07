use kah_cli::observer_review::queue::preparation_coverage::{Authority, Coverage, covered};
use serde_json::json;
use std::collections::BTreeSet;
fn authority(revision: u64) -> Authority {
    Authority::from_intent(&json!({"revision":revision,"goal":"Verbatim user goal","items":[["user-one","Whole user words"]]})).unwrap()
}
fn coverage(revision: u64, ids: &[&str]) -> Coverage {
    Coverage {
        worker: "worker".into(),
        reference: "queued-drift".into(),
        authority: authority(revision),
        action_ids: ids.iter().map(|s| s.to_string()).collect(),
    }
}
#[test]
fn only_full_same_worker_and_same_or_later_authority_ranges_are_covered() {
    let actions = BTreeSet::from(["a".into(), "b".into()]);
    assert!(covered(
        "worker",
        &authority(3),
        &actions,
        &coverage(3, &["a", "b", "c"])
    ));
    assert!(covered(
        "worker",
        &authority(3),
        &actions,
        &coverage(4, &["a", "b", "c"])
    ));
    assert!(!covered(
        "worker",
        &authority(3),
        &actions,
        &coverage(3, &["a"])
    ));
    assert!(!covered(
        "worker",
        &authority(4),
        &actions,
        &coverage(3, &["a", "b"])
    ));
    assert!(!covered(
        "worker",
        &authority(3),
        &BTreeSet::from(["newer".into()]),
        &coverage(3, &["a", "b"])
    ));
    assert!(!covered(
        "other",
        &authority(3),
        &actions,
        &coverage(3, &["a", "b"])
    ));
    assert!(!covered(
        "worker",
        &authority(3),
        &BTreeSet::new(),
        &coverage(3, &["a", "b"])
    ));
}
#[test]
fn missing_revisions_require_exact_authority_equality_not_a_guessed_order() {
    let old = Authority::from_intent(
        &json!({"goal":"Verbatim user goal","items":[["user-one","Whole user words"]]}),
    )
    .unwrap();
    let exact = Authority::from_intent(
        &json!({"goal":"Verbatim user goal","items":[{"id":"user-one","text":"Whole user words"}]}),
    )
    .unwrap();
    let changed = Authority::from_intent(
        &json!({"goal":"Verbatim user goal","items":[["user-one","DIFFERENT words"]]}),
    )
    .unwrap();
    let ids = BTreeSet::from(["a".into()]);
    let mut full = coverage(3, &["a", "b"]);
    full.authority = exact;
    assert!(covered("worker", &old, &ids, &full));
    full.authority = changed;
    assert!(!covered("worker", &old, &ids, &full));
}
