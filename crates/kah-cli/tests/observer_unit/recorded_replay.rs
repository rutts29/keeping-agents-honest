//! VF14's whole native command is evidence, never a replayable project check.
use super::*;

#[test]
fn retained_mixed_worker_command_uses_the_whole_v12_plan_without_replay() {
    let value: Value = serde_json::from_str(include_str!(
        "../../../../docs/gates/evidence/phase3-verifier-replay/retained-input.json"
    ))
    .unwrap();
    let request: Request = serde_json::from_value(value["request"].clone()).unwrap();
    let report: Report = serde_json::from_value(value["report"].clone()).unwrap();
    let records = replay::dispositions(&request, &report).unwrap();
    let items = records.as_array().unwrap();
    assert!(!items.is_empty());
    for record in items {
        let command = record["command"].as_str().unwrap();
        assert!(command.contains("cat > normalize.cjs"));
        assert!(command.contains("git commit"));
        assert!(command.contains("git tag"));
        assert_eq!(record["status"], "not_replayable");
        assert!(
            record["reason"]
                .as_str()
                .unwrap()
                .starts_with("REVIEW_RECORDED_CHECK_NOT_REPLAYABLE")
        );
    }
    let verification = items
        .iter()
        .find(|r| r["requirement_id"] == "verification-pass-line")
        .unwrap();
    assert_eq!(
        verification["verification_plan"]["method"],
        json!({"kind":"targeted_check","command":"node verification.cjs | grep -qx 'verification ok'"})
    );
    let proof = verify_commands(request.head.as_deref().unwrap(), &[], vec![], |_, _| {
        panic!("mixed command was replayed")
    })
    .unwrap();
    assert!(proof.tests.is_empty());
}
