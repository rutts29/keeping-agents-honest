//! Each live checkpoint outcome is saved before later journey assertions can
//! fail. A labelled follow-up never replaces an earlier result.
use super::*;

pub(super) fn record(provider: &str, stage: &str, result: &Value) {
    let followup = std::env::var("KAH_GATE_CHECKPOINT_FOLLOWUP").ok();
    if let Some(label) = &followup {
        assert!(
            label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid follow-up label"
        );
    }
    let expected = stage != "broken";
    let question_set = &result["verified_question_set"];
    let mut evidence = json!({"provider":provider,"stage":stage,"followup":followup,
        "questionSet":question_set,"expectedPass":expected,
        "matchesExpected":(result["assessment"]["passed"]==true)==expected,"nativeReport":result});
    crate::privacy::scrub(&mut evidence);
    let label = followup.map_or_else(|| "first".into(), |label| format!("followup-{label}"));
    let path = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
        .join(format!("checkpoint-{label}-{provider}-{stage}.json"));
    assert!(
        !path.exists(),
        "never overwrite an earlier checkpoint outcome"
    );
    fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
}
