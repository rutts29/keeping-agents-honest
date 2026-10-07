use super::super::followup::{Attempt, Binding};
use super::*;
fn result(id: &str, verified: bool, needs_check: bool) -> ItemResult {
    ItemResult {
        binding: Binding {
            worker: "worker".into(),
            review: "review-contract".into(),
            head: "523af6fcbf1990e2764bf7f28ced10c59d0d597e".into(),
            intent_revision: 2,
        },
        requirement_id: id.into(),
        claim: format!("Exact original contract {id}"),
        plan_digest: "fixture-plan".into(),
        verified,
        needs_check,
        semantic_risk_admissible: !verified && !needs_check,
        coverage: json!({"covered":verified}),
        check: None,
        tried: vec![Attempt {
            what: format!("Checked the source and receipt specific to {id}."),
            receipt: format!("{id}-receipt"),
        }],
    }
}
#[test]
fn exact_eleven_item_case_leaves_only_semantic_candidates_not_verification_work() {
    let units: kah_observer::decision_units::ReviewUnits = serde_json::from_str(include_str!(
        "../../../kah-observer/tests/fixtures/v9-eleven-unverified-units.json"
    ))
    .unwrap();
    let ids = units
        .requirements
        .iter()
        .map(|r| r.id.clone())
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 11);
    let results = ids
        .iter()
        .map(|id| {
            result(
                id,
                !matches!(id.as_str(), "checkpoint_intent" | "checkpoint_drift"),
                false,
            )
        })
        .collect::<Vec<_>>();
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":ids});
    apply_items(&results, &mut assessment).unwrap();
    assert_eq!(
        assessment["needs_thinker"],
        json!(["checkpoint_intent", "checkpoint_drift"])
    );
    assert_eq!(assessment["contract_verified"].as_array().unwrap().len(), 9);
    assert!(assessment["needs_check"].as_array().unwrap().is_empty());
}
#[test]
fn uncovered_executable_item_sends_work_back_without_question_or_stalled_round() {
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["normalize_contract"]});
    apply_items(
        &[result("normalize_contract", false, true)],
        &mut assessment,
    )
    .unwrap();
    assert_eq!(assessment["passed"], false);
    assert_eq!(assessment["state"], "checkpoint_needs_check");
    assert!(assessment["needs_user"].as_array().unwrap().is_empty());
    assert_eq!(assessment["needs_check"][0]["id"], "normalize_contract");
}
#[test]
fn covered_all_means_no_user_question_but_violations_still_win() {
    let results = [result("normalize_contract", true, false)];
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["normalize_contract"]});
    apply_items(&results, &mut assessment).unwrap();
    assert_eq!(assessment["passed"], true);
    assessment = json!({"passed":false,"violations":["scope"],"needs_user":[],"needs_thinker":["normalize_contract"]});
    apply_items(&results, &mut assessment).unwrap();
    assert_eq!(assessment["passed"], false);
}

#[test]
fn named_send_back_result_releases_pending_without_creating_a_user_wait() {
    use crate::observer_review::{
        consume, questions,
        state::{Pending, Purpose, Request, State, Store},
    };
    let root = std::env::temp_dir().join(format!(
        "kah-send-back-lease-{}",
        crate::relay_store::nonce()
    ));
    let mut store = Store::delivery_at(&root.join("test-owner"), true).unwrap();
    store.directory = root.join("review");
    kah_core::run_store::ensure_private_directory(&store.directory).unwrap();
    let request = Request {
        id: "review-send-back".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 1,
    };
    let mut state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":"worker","project":root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    state.pending = Some(Pending::from(&request));
    let mut assessment = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["normalize"],"unproven":[]});
    apply_items(&[result("normalize", false, true)], &mut assessment).unwrap();
    consume::commit_result(
        &store,
        &mut state,
        &request,
        json!({"request":request.id,"head":request.head,"assessment":assessment}),
        None,
    )
    .unwrap();
    let saved = store.load().unwrap().unwrap();
    assert!(saved.pending.is_none());
    assert_eq!(saved.checkpoint.unwrap()["state"], "checkpoint_needs_check");
    assert!(questions::pending(&store.directory).unwrap().is_empty());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
