use kah_cli::observer_review::{
    state::{Purpose, Request},
    verdict_cache::projection,
};
use serde_json::json;
use std::path::Path;

#[test]
fn clean_authority_is_referenced_dirty_words_remain_whole_and_host_intent_is_unchanged() {
    let whole = "A complete user sentence.\n".repeat(2000);
    let request = Request {
        id: "review-cache".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Original goal words.","items":[
            {"id":"clean","text":whole},{"id":"dirty","text":"Do not change this behavior."}]}),
        trigger: json!({"review_class":"commit","cache_plan":{"protocol":"commit-cache-v1",
            "carried_authority_items":["clean","dirty"],"fresh_authority_items":["goal","dirty"]}}),
        documents: vec![],
        created_ms: 0,
    };
    let before = request.intent.clone();
    let view = projection::intent(&request, Path::new("/input/intent-full.json")).unwrap();
    assert_eq!(view["goal"], request.intent["goal"]);
    assert_eq!(
        view["items"],
        json!([{"id":"dirty","text":"Do not change this behavior."}])
    );
    assert_eq!(view["carried_items"][0]["id"], "clean");
    assert!(
        view["carried_items"][0]["digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:")
    );
    assert_eq!(view["full_record"], "/input/intent-full.json");
    assert_eq!(request.intent, before);
}

#[test]
fn gates_and_uncached_authority_keep_the_complete_words() {
    let mut request = Request {
        id: "review-gate".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: json!({"goal":"Original goal","items":[{"id":"new","text":"Whole new user words."}]}),
        trigger: json!({"review_class":"gate"}),
        documents: vec![],
        created_ms: 0,
    };
    assert_eq!(
        projection::intent(&request, Path::new("/full")).unwrap(),
        request.intent
    );
    request.trigger = json!({"review_class":"commit","cache_plan":{"protocol":"commit-cache-v1",
        "carried_authority_items":[],"fresh_authority_items":[]}});
    let view = projection::intent(&request, Path::new("/full")).unwrap();
    assert_eq!(view["items"], request.intent["items"]);
}

#[test]
fn a_pending_user_input_stays_verbatim_even_if_its_id_matches_a_carry() {
    let request = Request {
        id: "review-pending".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: json!({"goal":"Original goal","items":[
            {"id":"pending","text":"Whole pending words.","status":"pending_verification"}]}),
        trigger: json!({"review_class":"commit","cache_plan":{"protocol":"commit-cache-v1",
            "carried_authority_items":["pending"],"fresh_authority_items":[]}}),
        documents: vec![],
        created_ms: 0,
    };
    let view = projection::intent(&request, Path::new("/full")).unwrap();
    assert_eq!(view["items"], request.intent["items"]);
    assert_eq!(view["carried_items"], json!([]));
}
