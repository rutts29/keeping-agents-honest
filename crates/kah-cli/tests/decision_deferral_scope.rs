use kah_cli::observer_review::{
    decision_deferral_scope,
    state::{Purpose, Request},
};
use serde_json::json;
use std::collections::BTreeSet;
#[test]
fn all_active_authority_stays_verbatim_and_only_touched_deferrals_are_inlined() {
    let request = Request {
        id: "review-scoped".into(),
        worker: "worker".into(),
        purpose: Purpose::Drift,
        turn: "turn".into(),
        head: None,
        intent: json!({"goal":"Whole goal.","items":[
            {"id":"user-goal","text":"Build this."},{"id":"user-prohibition","text":"Never commit notes/."}],
            "open_deferrals":[{"id":"deferral-a","requirement":{"quote":"Whole original anchor."}},
                {"id":"deferral-b","requirement":{"quote":"Whole unrelated historical anchor."}}]}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let state = json!({"intent":request.intent,"coverage":{"item":{"requirement_id":"a"}}});
    let result = decision_deferral_scope::scope(
        &request,
        state,
        &BTreeSet::from(["deferral-a".into()]),
        std::path::Path::new("/public/intent.json"),
    )
    .unwrap();
    assert_eq!(result["intent"]["goal"], request.intent["goal"]);
    assert_eq!(result["intent"]["items"], request.intent["items"]);
    assert_eq!(
        result["intent"]["open_deferrals"],
        json!([request.intent["open_deferrals"][0]])
    );
    assert_eq!(result["intent"]["other_deferrals"][0]["id"], "deferral-b");
    assert_eq!(
        result["intent"]["other_deferrals"][0]["pointer"],
        "/open_deferrals/1"
    );
    assert_eq!(
        result["intent"]["other_deferrals"][0]["full_source"],
        "/public/intent.json"
    );
    assert!(
        !result
            .to_string()
            .contains("Whole unrelated historical anchor.")
    );
    assert!(
        decision_deferral_scope::scope(
            &request,
            json!({}),
            &BTreeSet::from(["unknown".into()]),
            std::path::Path::new("/public/intent.json")
        )
        .is_err()
    );
}
