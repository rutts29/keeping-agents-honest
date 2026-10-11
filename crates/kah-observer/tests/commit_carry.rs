use kah_observer::commit_review::{DoubtLine, NativeVerdict, checkpoint, classify, recalibration};
use serde_json::json;
use std::collections::BTreeMap;
#[test]
fn checkpoints_and_tags_carry_only_unchanged_direct_reviews_and_keep_interaction_scope() {
    let policy: DoubtLine =
        serde_json::from_value(json!({"line":0.5,"provenance":["data"],"adjustment":"human"}))
            .unwrap();
    let mut direct = classify(
        "worker",
        "sha1",
        &[json!({"request":"r","question":"q","noul":0.7})],
        &policy,
    )
    .unwrap();
    direct.native = Some(NativeVerdict {
        review: "native1".into(),
        observer_session: "same".into(),
        authority_digest: "authority".into(),
        files: BTreeMap::from([("a.rs".into(), "digest".into())]),
        rows: vec![],
    });
    let quiet = classify(
        "worker",
        "sha2",
        &[json!({"request":"r2","question":"q","noul":0.1})],
        &policy,
    )
    .unwrap();
    let files = BTreeMap::from([("a.rs".into(), "digest".into())]);
    let plan = checkpoint::plan(&[direct.clone(), quiet.clone()], "authority", &files);
    assert_eq!(plan.carried.len(), 1);
    assert_eq!(plan.review_commits, vec!["sha2"]);
    assert_eq!(plan.interactions, vec!["sha1", "sha2"]);
    assert!(
        checkpoint::plan(&[direct.clone()], "new-authority", &files)
            .carried
            .is_empty()
    );
    assert!(
        checkpoint::plan(&[direct.clone()], "authority", &BTreeMap::new())
            .carried
            .is_empty()
    );
    assert_eq!(
        checkpoint::folded(&direct, Some("tag-review")).unwrap()["covered_by_checkpoint"],
        "tag-review"
    );
    assert!(checkpoint::folded(&quiet, Some("tag-review")).is_none());
    let report = recalibration::report(&[direct, quiet], 0.8).unwrap();
    assert_eq!(report["policy_changed"], false);
    assert_eq!(report["rows"][0]["would_wake"], false);
    assert_eq!(policy.line, 0.5);
}
#[test]
fn a_later_range_violation_does_not_relabel_an_older_good_commit() {
    let policy: DoubtLine =
        serde_json::from_value(json!({"line":0.5,"provenance":["data"],"adjustment":"human"}))
            .unwrap();
    let mut older = classify("worker", "old", &[], &policy).unwrap();
    let mut current = classify("worker", "new", &[], &policy).unwrap();
    let rows = vec![kah_observer::commit_check::Row {
        requirement: "rule".into(),
        finding: "new code violates rule".into(),
        status: kah_observer::commit_check::Status::Violated,
        kind: kah_observer::commit_check::FindingKind::Behavior,
        quotes: vec![],
    }];
    checkpoint::record(&mut older, "new", "gate", &rows);
    checkpoint::record(&mut current, "new", "gate", &rows);
    assert!(older.checkpoints.is_empty());
    assert_eq!(current.checkpoints["gate"][0].requirement, "rule");
    let report = recalibration::report(&[older, current], 0.9).unwrap();
    assert_eq!(report["rows"][0]["unknown_label"], true);
    assert_eq!(
        report["rows"][1]["later_checkpoints"][0]["violations"],
        json!(["rule"])
    );
}
#[test]
fn a_counterfactual_line_keeps_mandatory_oversized_reviews() {
    let policy: DoubtLine =
        serde_json::from_value(json!({"line":0.5,"provenance":["data"],"adjustment":"human"}))
            .unwrap();
    let mut review = classify("worker", "head", &[], &policy).unwrap();
    review
        .gaps
        .push(json!({"route":"direct_observer_review","reason":"known_oversized"}));
    let report = recalibration::report(&[review], 0.99).unwrap();
    assert_eq!(report["rows"][0]["would_wake"], true);
    assert_eq!(report["policy_changed"], false);
}
