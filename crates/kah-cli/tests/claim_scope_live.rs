use kah_cli::observer_review::claims::{ReviewClass, scope};
use kah_observer::judge::routing::{self, Backend};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

/// Authored native assistant messages and the review class each should route to.
fn cases() -> Vec<Value> {
    let case = |id: &str, expected: &str, text: &str| json!({"id":id,"expected":expected,"nativeSource":{"turn":format!("turn-{id}"),"native_assistant_text":text}});
    vec![
        case(
            "gate-phase-complete",
            "gate",
            "Phase 2 of the execution plan is complete: every journey in the phase passed and the gate evidence is saved.",
        ),
        case(
            "gate-passed",
            "gate",
            "Gate 3 has passed. All of its live journeys ran green and the gate report is written.",
        ),
        case(
            "commit-slice-done",
            "commit",
            "I fixed the retry bug and committed it. The change is checked in; the phase itself is still in progress.",
        ),
        case(
            "plain-ack",
            "none",
            "Understood. I am ready and waiting for the next instruction.",
        ),
    ]
}

#[test]
#[ignore = "paid live Jev routing; run explicitly with --ignored and KAH_GATE_EVIDENCE set"]
fn native_claims_route_to_the_user_authorized_classes() {
    let cases = cases();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let output =
        std::env::var_os("KAH_GATE_EVIDENCE").map_or_else(std::env::temp_dir, PathBuf::from);
    let mut outcomes = vec![];
    let mut failures = vec![];
    for case in &cases {
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
        "Routing misses: {failures:?}; do not wire or tune silently"
    );
}
