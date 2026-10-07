//! Frozen ordinary-ack negative from track2; use the EXACT product question.
//! Real Jev, no agent/headless session, no wording or threshold tuning.
use kah_cli::observer_review::claims;
use kah_observer::judge::routing::{self, Backend};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
#[test]
#[ignore = "real Jev claim screen; make gate only after the current live gate ends"]
fn frozen_ordinary_ack_does_not_route_to_a_gate() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let freeze: Value = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase-5o-efficiency-b/cases-frozen.json"
    ))
    .unwrap();
    let case = freeze["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "plain-startup-ack-is-not-a-gate")
        .unwrap();
    let state =
        json!({"turn":"retained-startup-ack","native_assistant_text":case["nativeAssistantText"]});
    let judgment = routing::evaluate(
        &home,
        Backend::Jev,
        "jev-latest",
        &state,
        &claims::questions(),
    );
    let outcome = match &judgment {
        Ok(j) => {
            json!({"judgment":j,"routesToGate":claims::review_class(j)==Some(claims::ReviewClass::Gate),"claimScreenTriggered":claims::needs_review(j)})
        }
        Err(e) => json!({"error":e.to_string()}),
    };
    let root = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(root.join(format!("ack-{}.json",kah_cli::relay_store::nonce())),serde_json::to_vec_pretty(&json!({"case":case,"state":state,"outcome":outcome,"questionSet":claims::VERSION,"questionsUnchanged":true,"thresholdsUnchanged":true})).unwrap()).unwrap();
    assert_eq!(
        outcome["claimScreenTriggered"], false,
        "Retain miss and stop; no thresholds/wording iteration"
    );
}
