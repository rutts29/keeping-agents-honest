//! Fresh native captures, registered before question wording. Never tune against
//! their outcomes; save each result before later journey assertions can fail.
use super::*;
use sha2::{Digest, Sha256};

pub(super) fn record(provider: &str, stage: &str, result: &Value) {
    if super::checkpoints::v5_enabled() {
        return super::checkpoint_v5_heldout::record(provider, stage, result);
    }
    if super::checkpoints::v4_enabled() {
        return record_v4(provider, stage, result);
    }
    let (case, expected) = match (provider, stage) {
        ("codex", "first") => ("genuine-future-dependency", true),
        ("codex", "broken") => ("broken-integration", false),
        ("claude", "broken") => ("fake-deferral", false),
        ("claude", "repaired") => ("repaired-work", true),
        _ => return,
    };
    let frozen =
        include_str!("../../../../docs/gates/evidence/phase-5o-c/checkpoint-cases-frozen.json");
    let registration: Value = serde_json::from_str(frozen).unwrap();
    let implementation = include_str!("../../../kah-observer/src/checkpoint.rs");
    assert_eq!(
        registration["question_freeze"]["sha256"],
        format!("{:x}", Sha256::digest(implementation)),
        "held-out questions changed after freeze"
    );
    let followup = std::env::var("KAH_GATE_CHECKPOINT_FOLLOWUP").ok();
    if let Some(label) = &followup {
        assert!(
            label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid trial label"
        );
    }
    let mut evidence = json!({
        "case":case,"provider":provider,"heldOut":followup.is_none(),"followup":followup,
        "caseRegistrationDigest":format!("sha256:{:x}", Sha256::digest(frozen)),
        "questionSet":kah_observer::checkpoint::VERSION,
        "expectedPass":expected,"matchesExpected":(result["assessment"]["passed"]==true)==expected,
        "nativeReport":result,
        "protocol":"Frozen wording; a labelled follow-up never replaces first held-out evidence."
    });
    crate::privacy::scrub(&mut evidence);
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap()).join(match &followup {
            Some(label) => format!("followup-{label}-{case}.json"),
            None => format!("held-out-{case}.json"),
        }),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}

fn record_v4(provider: &str, stage: &str, result: &Value) {
    let case = match stage {
        "first" => "available-parser-future-renderer",
        "broken" => "parser-renderer-fail-together",
        "repaired" => "parser-renderer-repaired",
        _ => return,
    };
    let registration: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase-5o-c/v4-cases-frozen.json"
    ))
    .unwrap();
    verify_v4_freeze(&registration);
    assert_eq!(
        result["verified_question_set"], 4,
        "new v4 capture used a legacy question set"
    );
    let expected = stage != "broken";
    let followup = std::env::var("KAH_GATE_CHECKPOINT_FOLLOWUP").ok();
    if let Some(label) = &followup {
        assert!(
            label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid follow-up label"
        );
    }
    let mut evidence = json!({"case":case,"provider":provider,"heldOut":followup.is_none(),"followup":followup,"questionSet":4,"expectedPass":expected,"matchesExpected":(result["assessment"]["passed"]==true)==expected,"nativeReport":result,"caseRegistration":registration,"protocol":"First fresh v4 capture or explicitly labelled follow-up; frozen wording/composition, keep all outcomes, no tuning."});
    crate::privacy::scrub(&mut evidence);
    let directory = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    let path = directory.join(followup.map_or_else(
        || format!("v4-held-out-{provider}-{case}.json"),
        |label| format!("v4-followup-{label}-{provider}-{case}.json"),
    ));
    assert!(!path.exists(), "never overwrite first v4 held-out result");
    fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}
fn verify_v4_freeze(registration: &Value) {
    let sources = [
        (
            "decision_questions",
            include_str!("../../../kah-observer/src/decision_units/evaluation.rs"),
        ),
        (
            "steering_questions",
            include_str!("../../../kah-observer/src/steering_proposal.rs"),
        ),
        (
            "composition",
            include_str!("../../src/observer_review/decisions.rs"),
        ),
        (
            "coverage",
            include_str!("../../src/observer_review/decisions/coverage.rs"),
        ),
        (
            "steering_application",
            include_str!("../../src/observer_review/decisions/steering.rs"),
        ),
    ];
    for (name, source) in sources {
        assert_eq!(
            registration["question_freeze"][name],
            format!("{:x}", Sha256::digest(source)),
            "v4 source changed after freeze: {name}"
        );
    }
}
