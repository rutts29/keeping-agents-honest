//! Keep each v7 outcome before any later assertion, with frozen source hashes.
use super::*;
use sha2::{Digest, Sha256};
pub fn record(provider: &str, stage: &str, result: &Value) {
    let registration: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase-5o-v7/cases-frozen.json"
    ))
    .unwrap();
    verify(&registration);
    assert_eq!(result["verified_question_set"], 7);
    let case = match stage {
        "first" => "csv-future-renderer",
        "broken" => "csv-broken-tagged-integration",
        _ => "csv-repaired-tagged-integration",
    };
    super::ambiguity::retain(
        &PathBuf::from(std::env::var_os("HOME").unwrap()),
        &format!("v7-{provider}-{stage}"),
        &json!({"case":case,"provider":provider,"stage":stage,"expectedPass":stage!="broken","passed":result["assessment"]["passed"],"result":result,"registration":registration}),
    );
}
pub(super) fn verify(registration: &Value) {
    assert_eq!(
        registration["question_freeze"]["frozen_before_live_captures"],
        true
    );
    for (path, source) in [
        (
            "crates/kah-observer/src/decision_units/evaluation.rs",
            include_str!("../../../kah-observer/src/decision_units/evaluation.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/applicability.rs",
            include_str!("../../../kah-observer/src/decision_units/applicability.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/blocker.rs",
            include_str!("../../../kah-observer/src/decision_units/blocker.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/completion.rs",
            include_str!("../../../kah-observer/src/decision_units/completion.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/workspace/v7.rs",
            include_str!("../../src/observer_review/workspace/contract.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/workspace/instructions.rs",
            include_str!("../../src/observer_review/workspace/instructions.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/completion_due.rs",
            include_str!("../../src/observer_review/completion_due.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/decisions.rs",
            include_str!("../../src/observer_review/decisions.rs"),
        ),
    ] {
        assert_eq!(
            registration["question_freeze"]["sources"][path],
            format!("{:x}", Sha256::digest(source)),
            "v7 source changed after freeze: {path}"
        );
    }
}
