use kah_observer::{
    decision_units::{self, ReviewUnits},
    judge::{Judgment, Metrics},
};
use std::collections::BTreeMap;
fn captured() -> ReviewUnits {
    serde_json::from_str(include_str!("fixtures/synthetic-v9-eleven-units.json")).unwrap()
}
fn empty_judgment() -> Judgment {
    Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::new(),
        metrics: Metrics::default(),
        fallback_reason: None,
    }
}
#[test]
fn atomic_receipts_do_not_need_semantic_timing_or_a_user_decision() {
    let mut units = captured();
    units.version = Some(10);
    let atomic = [
        "verification_pass_line",
        "verification_receipt_status",
        "delivery_sha",
    ];
    let questions = decision_units::questions(&units);
    let outcome = decision_units::assess(&units, &empty_judgment());
    for id in atomic {
        assert!(!questions.contains_key(&format!("applicability.{id}")));
        assert!(!questions.contains_key(&format!("requirement.{id}")));
        assert!(!outcome.needs_thinker.iter().any(|x| x == id));
        assert!(!outcome.needs_user.iter().any(|x| x == id));
    }
    assert!(!outcome.passed); // The remaining semantic/contract work still waits.
}
#[test]
fn a_v9_case_still_records_its_actual_literal_timing_failure() {
    let units = captured();
    let outcome = decision_units::assess(&units, &empty_judgment());
    assert!(outcome.needs_thinker.contains(&"delivery_sha".into()));
}
