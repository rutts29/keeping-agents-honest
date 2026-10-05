//! Frozen Claude case; Codex mirrors are labelled development evidence.
use super::*;
use sha2::{Digest, Sha256};
pub fn record(provider: &str, stage: &str, result: &Value) {
    let registration: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase-5o-c/v6-cases-frozen.json"
    ))
    .unwrap();
    verify(&registration);
    assert_eq!(result["verified_question_set"], 6);
    let followup = std::env::var("KAH_GATE_CHECKPOINT_FOLLOWUP").ok();
    if let Some(label) = &followup {
        assert!(
            label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        );
    }
    let mut evidence = json!({"case":"claude-json-roundtrip-checkpoints","stage":stage,"provider":provider,"heldOut":provider=="claude"&&followup.is_none(),"developmentMirror":provider!="claude","followup":followup,"questionSet":6,"expectedPass":stage!="broken","matchesExpected":result["assessment"]["passed"]==json!(stage!="broken"),"nativeReport":result,"caseRegistration":registration,"protocol":"Frozen v6 case; Codex is an explicitly labelled development mirror; every outcome retained."});
    crate::privacy::scrub(&mut evidence);
    let label = followup.map_or_else(|| "first".into(), |label| format!("followup-{label}"));
    let path = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
        .join(format!("v6-{label}-{provider}-{stage}.json"));
    assert!(!path.exists(), "never overwrite a v6 result");
    fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}
pub(super) fn verify(registration: &Value) {
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
            "crates/kah-observer/src/decision_units/artifact.rs",
            include_str!("../../../kah-observer/src/decision_units/artifact.rs"),
        ),
        (
            "crates/kah-observer/src/decision_units/validation.rs",
            include_str!("../../../kah-observer/src/decision_units/validation.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/workspace/instructions.rs",
            include_str!("../../src/observer_review/workspace/instructions.rs"),
        ),
        (
            "crates/kah-cli/src/observer_review/decisions.rs",
            include_str!("../../src/observer_review/decisions.rs"),
        ),
    ] {
        assert_eq!(
            registration["question_freeze"]["sources"][path],
            format!("{:x}", Sha256::digest(source)),
            "v6 source changed after freeze: {path}"
        );
    }
}
