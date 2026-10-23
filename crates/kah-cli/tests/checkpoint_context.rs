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
fn named_scope_is_a_literal_path_in_the_shared_decision_state() {
    let scope =
        json!({"phase":"Phase 2","components":["parser","renderer"],"authorization_citation":0});
    let actions = json!([{"evidence":{"topic":"integration","checkpoint_scope":scope,"citations":[{"quote":"Phase 2 integrates parser and renderer."}]}}]);
    let prepared = kah_cli::observer_review::checkpoint::decision_context(
        &json!({"goal":"exact user words"}),
        &json!({}),
        Some("sha"),
        &actions,
        json!([]),
    );
    assert_eq!(prepared["event"]["checkpoint_scope"], scope);
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
