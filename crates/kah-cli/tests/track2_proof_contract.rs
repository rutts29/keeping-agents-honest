#[path = "track2_support/proof_contract.rs"]
mod contract;
use kah_observer::judge::{Answer, Judgment, Metrics};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn result(pass: bool, sha: &str, units: &[(&str, &str)]) -> Value {
    let answers = units
        .iter()
        .enumerate()
        .map(|(i, _)| {
            (
                format!("requirement.unit{i}"),
                Answer::Choice {
                    choice: "supported".into(),
                    probabilities: BTreeMap::from([("supported".into(), 1.0)]),
                    confidence: 1.0,
                },
            )
        })
        .collect();
    let judgment = Judgment {
        backend: "jev".into(),
        model: "jev-1.13.0".into(),
        answers,
        metrics: Metrics {
            calls: 1,
            ..Metrics::default()
        },
        fallback_reason: None,
    };
    json!({"request":format!("r-{sha}"),"head":sha,"observer_session":"same","pane":"same-pane",
        "native_tokens":12,"usage_gap":null,"assessment":{"passed":pass},"judgment":judgment,
        "report":{"decision_units":{"requirements":units.iter().enumerate().map(|(i,(dimension,decision))|
            json!({"id":format!("unit{i}"),"dimension":dimension,"judgment":{"kind":"semantic","decision":decision}})).collect::<Vec<_>>()}}})
}
fn fixture() -> (Value, Vec<Value>) {
    let replay = json!([{"counterfactual":false,"result":result(true,"actual",&[])},{"counterfactual":true,"result":result(false,"synthetic",&[("drift","violated")])}]);
    let stages = vec![
        json!({"result":result(true,"one",&[("deferral","deferred")]),"probe":{"sha":"one","phase1":true,"serializer_absent":true},"deferrals":[{"id":"serializer"}],"phase3_started":false}),
        json!({"result":result(false,"two",&[("deferral","violated"),("integration","violated")]),"probe":{"sha":"two","phase1":true,"phase2":true,"integration":false},"deferrals":[],"native_nudge":true,"nudge_receipt":{"path":"private-receipt"},"phase3_started":false}),
        json!({"result":result(true,"three",&[]),"probe":{"sha":"three","phase1":true,"phase2":true,"integration":true},"deferrals":[],"phase3_started":false}),
    ];
    (replay, stages)
}
#[test]
fn proof_needs_actual_receipts_repair_and_validated_deferral_decisions() {
    let (replay, stages) = fixture();
    assert_eq!(contract::verify(&replay, &stages), Ok(()));
    for key in ["native_tokens", "judgment", "head", "pane"] {
        let mut bad = stages.clone();
        bad[2]["result"].as_object_mut().unwrap().remove(key);
        assert!(contract::verify(&replay, &bad).is_err(), "missing {key}");
    }
    let mut bad = stages.clone();
    bad[2]["probe"]["integration"] = false.into();
    assert!(contract::verify(&replay, &bad).is_err());
    let mut bad = stages.clone();
    bad[1]["result"]["judgment"]["answers"]["requirement.unit0"]["confidence"] = 0.4.into();
    assert_eq!(
        contract::verify(&replay, &bad),
        Err("TRACK2_FAKE_OR_INTEGRATION_UNCONFIRMED")
    );
    let mut bad = stages.clone();
    bad[0]["result"]["report"]["decision_units"]["requirements"][0]["judgment"]["decision"] =
        "satisfied".into();
    assert_eq!(
        contract::verify(&replay, &bad),
        Err("TRACK2_GENUINE_NOT_RECORDED")
    );
}
#[test]
fn missing_nudge_and_relabelled_counterfactual_never_pass() {
    let (mut replay, mut stages) = fixture();
    replay[1]["counterfactual"] = false.into();
    assert_eq!(
        contract::verify(&replay, &stages),
        Err("TRACK2_REPLAY_CHRONOLOGY")
    );
    replay[1]["counterfactual"] = true.into();
    stages[1].as_object_mut().unwrap().remove("nudge_receipt");
    assert_eq!(
        contract::verify(&replay, &stages),
        Err("TRACK2_NUDGE_MISSING")
    );
}
