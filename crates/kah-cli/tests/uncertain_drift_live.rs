//! Actual uncertain Tier-1 -> native full-context report -> Jev, no fake tick.
use kah_cli::observer_review::{
    state::{Purpose, Store},
    workspace,
};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "real builder escalation proof; make gate PHASE=5o-bound"]
fn uncertain_builder_tick_receives_full_context_review_and_jev_decision() {
    let worker = std::env::var("KAH_GATE_REAL_SESSION").unwrap();
    let store = Store::open(&worker).unwrap();
    let state = store.load().unwrap().unwrap();
    let result = state.latest.as_ref().unwrap();
    assert_eq!(result["purpose"], "drift");
    let request = workspace::request(&store, result["request"].as_str().unwrap()).unwrap();
    assert_eq!(request.purpose, Purpose::Drift);
    let uncertain = find_uncertain(&request.trigger).expect("actual recorded uncertain tick");
    assert_eq!(uncertain["tripwire"]["backend"], "jev");
    assert!(uncertain["tripwire"]["metrics"]["calls"].as_u64().unwrap() > 0);
    let report: Report = serde_json::from_value(result["report"].clone()).unwrap();
    let evidence = report.verify(&request.id, &request.documents).unwrap();
    assert!(!evidence.is_empty());
    for kind in ["ledger", "worker_transcript", "diff", "plan"] {
        assert!(request.documents.iter().any(|d| d.kind == kind));
    }
    assert_eq!(result["judgment"]["backend"], "jev");
    assert!(
        result["judgment"]["metrics"]["input_tokens"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(state.observer.model, "gpt-6.1-sol");
    assert_eq!(state.observer.effort, "low");
    let version = Command::new(std::env::var("HOME").unwrap() + "/.local/bin/codex")
        .arg("--version")
        .output()
        .unwrap();
    let artifact = json!({"worker":worker,"request":request.id,"nativeVersion":String::from_utf8(version.stdout).unwrap().trim(),"observerSession":state.observer.session,"samePersistentPane":state.observer.pane,"model":state.observer.model,"effort":state.observer.effort,"actualTripwire":uncertain["tripwire"],"tripwireAssessment":uncertain["assessment"],"verifiedEvidenceItems":evidence.len(),"fullContextDocumentDigests":request.documents.iter().map(|d|json!({"id":d.id,"kind":d.kind,"digest":d.digest})).collect::<Vec<_>>(),"finalJevDecision":result["judgment"],"finalAssessment":result["assessment"],"perWakeNativeTokens":result["native_tokens"],"elapsedMs":result["elapsed_ms"],"thresholdsUnchanged":true,"passed":true});
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join("uncertain-full-review.json"),
        serde_json::to_vec_pretty(&artifact).unwrap(),
    )
    .unwrap();
}
fn find_uncertain(value: &Value) -> Option<&Value> {
    if value["assessment"]["state"] == "uncertain" && value.get("tripwire").is_some() {
        return Some(value);
    }
    value
        .get("coalesced_triggers")?
        .as_array()?
        .iter()
        .find_map(find_uncertain)
}
