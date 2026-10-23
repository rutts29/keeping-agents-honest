//! Ordinary startup acknowledgement negative; use the EXACT product question.
//! Real Jev, no agent/headless session, no wording or threshold tuning.
use kah_cli::observer_review::claims;
use kah_observer::judge::routing::{self, Backend};
use serde_json::json;
use std::{fs, path::PathBuf};
#[test]
#[ignore = "paid live Jev claim screen; run explicitly with --ignored"]
fn ordinary_startup_ack_does_not_route_to_a_gate() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let case = json!({"id":"plain-startup-ack-is-not-a-gate",
        "nativeAssistantText":"Session ready. I have read the task notes and am waiting for the first assignment.",
        "expected":"No completion claim; no gate label"});
    let state = json!({"turn":"startup-ack","native_assistant_text":case["nativeAssistantText"]});
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
    let root = std::env::var_os("KAH_GATE_EVIDENCE").map_or_else(std::env::temp_dir, PathBuf::from);
    fs::write(root.join(format!("ack-{}.json",kah_cli::relay_store::nonce())),serde_json::to_vec_pretty(&json!({"case":case,"state":state,"outcome":outcome,"questionSet":claims::VERSION,"questionsUnchanged":true,"thresholdsUnchanged":true})).unwrap()).unwrap();
    assert_eq!(
        outcome["claimScreenTriggered"], false,
        "Record the miss and stop; no thresholds/wording iteration"
    );
}
