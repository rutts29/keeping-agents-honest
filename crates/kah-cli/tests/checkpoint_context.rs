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
fn a_native_checkpoint_keeps_every_authority_item_and_selected_citation() {
    let plan = "Phase 2 adds the renderer after the parser is complete.";
    let intent = json!({"revision":3,"goal":"Build a CSV parser now and a renderer in phase 2.",
        "items":[{"id":"user-1","text":"Keep quoted fields exactly as written."},
            {"id":"user-2","text":"Do not start the renderer before phase 2."}],
        "failures":[{"head":"old","output":"an earlier parser test failed"}],"open_deferrals":[]});
    let deferral = json!({"id":"renderer","phase":"Phase 2","introduced_at":"review-1700000100000-1",
        "requirement":{"source":"plan-0","uri":"PLAN.md","kind":"plan","quote":plan},
        "dependency":{"source":"plan-0","uri":"PLAN.md","kind":"plan","quote":plan}});
    let actions = json!([{"id":"verified-0","kind":"verifiedEvidence","evidence":{"topic":"integration",
        "model_claim_not_a_decision":"The parser tests pass on this head.",
        "citations":[{"source":"native:check","quote":"PARSER_TESTS_PASS","digest":"sha256:0c1d"}]}}]);
    let trigger = json!({"events":[{"kind":"user_gate","source":"host_cli"}]});
    let head = "7e9a1c3b5d7f9e1a3c5b7d9f1e3a5c7b9d1f3e5a";
    let cleaned = kah_cli::observer_review::checkpoint::decision_context(
        &intent,
        &trigger,
        Some(head),
        &actions,
        json!([deferral]),
    );
    assert_eq!(cleaned["intent"]["goal"], intent["goal"]);
    assert_eq!(cleaned["intent"]["items"], intent["items"]);
    assert_eq!(
        cleaned["event"],
        json!({"head":head,"trigger":trigger,"actions":actions,"open_and_proposed_deferrals":[deferral]})
    );
}
