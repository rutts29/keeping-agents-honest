//! Fresh native captures, registered before question wording. Never tune against
//! their outcomes; save each result before later journey assertions can fail.
use super::*;
use sha2::{Digest, Sha256};

pub(super) fn record(provider: &str, stage: &str, result: &Value) {
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
    let mut evidence = json!({
        "case":case,"provider":provider,"heldOut":true,
        "caseRegistrationDigest":format!("sha256:{:x}", Sha256::digest(frozen)),
        "questionSet":kah_observer::checkpoint::VERSION,
        "expectedPass":expected,"matchesExpected":(result["assessment"]["passed"]==true)==expected,
        "nativeReport":result,
        "protocol":"Captured after question freeze; no wording changes based on this outcome."
    });
    crate::privacy::scrub(&mut evidence);
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join(format!("held-out-{case}.json")),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
}
