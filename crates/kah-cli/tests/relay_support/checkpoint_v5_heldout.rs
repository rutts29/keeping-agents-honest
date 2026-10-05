//! New outcomes never overwrite v4, or the first v5 trial.
use super::*;
use sha2::{Digest, Sha256};
pub fn record(provider: &str, stage: &str, result: &Value) {
    let case = match stage {
        "first" => "percent-decoder-future-encoder",
        "broken" => "percent-roundtrip-fails-together",
        "repaired" => "percent-roundtrip-repaired",
        _ => return,
    };
    let registration: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase-5o-c/v5-cases-frozen.json"
    ))
    .unwrap();
    verify(&registration);
    assert_eq!(result["verified_question_set"], 5);
    let followup = std::env::var("KAH_GATE_CHECKPOINT_FOLLOWUP").ok();
    if let Some(label) = &followup {
        assert!(
            label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        );
    }
    let mut evidence = json!({"case":case,"provider":provider,"heldOut":followup.is_none(),"followup":followup,"questionSet":5,"expectedPass":stage!="broken","matchesExpected":result["assessment"]["passed"]==json!(stage!="broken"),"nativeReport":result,"caseRegistration":registration,"protocol":"Fresh v5 case; all prior failures retained; no tuning."});
    crate::privacy::scrub(&mut evidence);
    let name = followup.map_or_else(
        || format!("v5-held-out-{provider}-{case}.json"),
        |label| format!("v5-followup-{label}-{provider}-{case}.json"),
    );
    let path = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join(name);
    assert!(!path.exists(), "never overwrite an earlier v5 outcome");
    fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}
fn verify(registration: &Value) {
    for (path, source) in [
        (
            "crates/kah-observer/src/decision_units/evaluation.rs",
            include_str!("../../../kah-observer/src/decision_units/evaluation.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/literal.rs",
            include_str!("../../../kah-observer/src/decision_units/literal.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/validation.rs",
            include_str!("../../../kah-observer/src/decision_units/validation.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/schema/thinking.rs",
            include_str!("../../../kah-observer/src/decision_units/schema/thinking.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/workspace/instructions.rs",
            include_str!("../../src/observer_review/workspace/instructions.rs"),
        ),
    ] {
        assert_eq!(
            registration["questions_and_contract"][path],
            format!("{:x}", Sha256::digest(source)),
            "v5 source changed after freeze: {path}"
        );
    }
}
