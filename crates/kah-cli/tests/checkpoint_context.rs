use serde_json::json;
#[test]
fn current_checkpoint_uses_selected_evidence_without_reinjecting_old_failure_and_deferral_copies() {
    let intent = json!({"revision":3,"goal":"User words stay verbatim.","items":[{"id":"u","text":"Keep every user word."}],"failures":[{"head":"old","output":"old test failed"}],"open_deferrals":[{"id":"d","evidence":"old implementation failed"}]});
    let current = json!([{"id":"d","evidence":"verified current integration passes"}]);
    let actions = json!([{"evidence":{"topic":"integration","citations":[{"quote":"new-SHA INTEGRATION_PASS"}]}}]);
    let prepared = kah_cli::observer_review::checkpoint::decision_context(
        &intent,
        &json!({"kind":"user_gate"}),
        Some("new-SHA"),
        &actions,
        current.clone(),
    );
    assert_eq!(prepared["intent"]["goal"], intent["goal"]);
    assert_eq!(prepared["intent"]["items"], intent["items"]);
    assert!(prepared["intent"].get("failures").is_none());
    assert!(prepared["intent"].get("open_deferrals").is_none());
    assert_eq!(prepared["event"]["open_and_proposed_deferrals"], current);
    assert_eq!(prepared["event"]["actions"], actions);
}

#[test]
fn the_real_native_checkpoint_keeps_every_authority_item_and_selected_citation() {
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase-5o-c/failures/codex-after-tests-judgment.json"
    ))
    .unwrap();
    let s = &r["jev_state"];
    let cleaned = kah_cli::observer_review::checkpoint::decision_context(
        &s["intent"],
        &s["event"]["trigger"],
        s["event"]["head"].as_str(),
        &s["event"]["actions"],
        s["event"]["open_and_proposed_deferrals"].clone(),
    );
    assert_eq!(cleaned["intent"]["goal"], s["intent"]["goal"]);
    assert_eq!(cleaned["intent"]["items"], s["intent"]["items"]);
    assert_eq!(cleaned["event"], s["event"]);
}

#[test]
#[ignore = "live Jev replay of the exact native brain-prepared checkpoint; make gate only"]
fn live_exact_checkpoint_replay() {
    use kah_observer::judge::{
        Questions,
        routing::{self, Backend},
    };
    use std::{fs, path::PathBuf};
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let r: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/phase-5o-c/failures/codex-after-tests-judgment.json"
    ))
    .unwrap();
    let captured: serde_json::Value = serde_json::from_slice(
        &fs::read(r["assessment"]["private_replay"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let questions: Questions =
        serde_json::from_value(captured["request"]["questions"].clone()).unwrap();
    let s = &captured["request"]["state"];
    let state = kah_cli::observer_review::checkpoint::decision_context(
        &s["intent"],
        &s["event"]["trigger"],
        s["event"]["head"].as_str(),
        &s["event"]["actions"],
        s["event"]["open_and_proposed_deferrals"].clone(),
    );
    let judgment = routing::evaluate(
        &home,
        Backend::Jev,
        captured["request"]["model"].as_str().unwrap(),
        &state,
        &questions,
    )
    .unwrap();
    let outcome = kah_cli::observer_review::checkpoint::assess(
        &judgment,
        serde_json::from_value(state["event"]["open_and_proposed_deferrals"].clone()).unwrap(),
    );
    let deferrals_closed = outcome["deferrals"].as_array().unwrap().is_empty();
    let passed = outcome["passed"] == true && deferrals_closed;
    let evidence = serde_json::json!({"originalPrivateCapture":r["assessment"]["private_replay"],"originalJudgment":r["judgment"],"nativeBrainReportReused":true,"authorityVerbatim":state["intent"]["goal"]==s["intent"]["goal"]&&state["intent"]["items"]==s["intent"]["items"],"selectedEvidenceUnchanged":state["event"]==s["event"],"normalizedState":state,"judgment":judgment,"outcome":outcome,"deferralsClosed":deferrals_closed,"questionsAndThresholdsUnchanged":true,"passed":passed});
    fs::write(
        PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
            .join("exact-checkpoint-context-replay.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    assert!(
        passed,
        "actual replay remains unproven: {:?}",
        outcome["unproven"]
    );
}
