use kah_cli::observer_review::claims::{ReviewClass, scope};
use kah_observer::judge::routing::{self, Backend};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "real Jev replay; run only through make gate PHASE=5o-claim-routing"]
fn frozen_native_claims_route_to_the_user_authorized_classes() {
    let freeze: Value = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase-5o-claim-routing/cases-frozen.json"
    ))
    .unwrap();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let output = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    let mut outcomes = vec![];
    let mut failures = vec![];
    for case in freeze["cases"].as_array().unwrap() {
        let started = std::time::Instant::now();
        let judgment = routing::evaluate(
            &home,
            Backend::Jev,
            "jev-latest",
            &case["nativeSource"],
            &scope::questions(),
        );
        let actual = judgment.as_ref().ok().and_then(scope::review_class);
        let actual = match actual {
            Some(ReviewClass::Gate) => "gate",
            Some(ReviewClass::Commit) => "commit",
            None => "none",
        };
        let passed = judgment.is_ok() && case["expected"] == actual;
        if !passed {
            failures.push(case["id"].clone());
        }
        outcomes.push(json!({"id":case["id"],"expected":case["expected"],"actual":actual,"passed":passed,"judgment":judgment.as_ref().ok(),"error":judgment.as_ref().err().map(ToString::to_string),"elapsedMs":started.elapsed().as_millis()}));
        fs::write(output.join("replay.json"), serde_json::to_vec_pretty(&json!({"version":scope::VERSION,"questions":scope::questions(),"thresholdsUnchanged":true,"outcomes":outcomes,"failures":failures})).unwrap()).unwrap();
    }
    assert!(
        failures.is_empty(),
        "Frozen replay misses retained: {failures:?}; do not wire or tune silently"
    );
}
