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
        evidence: vec![],
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
fn confirmed_exemptions_remain_accounted_without_missing_check_or_verified_claim() {
    let items = [
        result("future", false, true),
        result("delivered", true, false),
    ];
    for key in ["not_due", "deferred"] {
        let mut a = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["delivered"],"unproven":["delivered"]});
        a[key] = json!(["future"]);
        apply_items(&items, &mut a).unwrap();
        assert_eq!(a["passed"], true);
        assert_eq!(a[key], json!(["future"]));
        assert_eq!(a["contract_verified"], json!(["delivered"]));
        assert!(a["needs_check"].as_array().unwrap().is_empty());
        assert!(a["unproven"].as_array().unwrap().is_empty());
    }
}

#[test]
fn exemption_no_longer_applicable_needs_a_check_instead_of_a_user_question() {
    let mut item = result("future", false, false);
    item.semantic_risk_admissible = false;
    let mut a = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["future"],"unproven":["future"]});
    apply_items(&[item], &mut a).unwrap();
    assert_eq!(a["passed"], false);
    assert_eq!(a["needs_check"][0]["id"], "future");
    assert!(a["needs_user"].as_array().unwrap().is_empty());
}

#[test]
fn final_confirmed_observations_resolve_stale_flags_but_executable_stays_blocking() {
    let items = [
        result("no-push", false, true),
        result("preserved", false, true),
        result("normalize", false, true),
    ];
    let mut a = json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["normalize"],"resolved_non_executable":["no-push"],"resolved_preservation":["preserved"]});
    apply_items(&items, &mut a).unwrap();
    assert_eq!(a["needs_check"].as_array().unwrap().len(), 1);
    assert_eq!(a["needs_check"][0]["id"], "normalize");
    assert_eq!(a["passed"], false);
    a["needs_thinker"] = json!([]);
    apply_items(&items[..2], &mut a).unwrap();
    assert!(a["needs_check"].as_array().unwrap().is_empty());
    assert_eq!(a["passed"], true);
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

#[test]
fn new_final_item_is_named_builder_work_and_never_a_user_risk() {
    let mut request = crate::observer_review::state::Request {
        id: "review-new-final".into(),
        worker: "worker".into(),
        purpose: crate::observer_review::state::Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"revision":1}),
        trigger: json!({"verification_followup":{"stage":"checks_complete","item_results":[]}}),
        documents: vec![],
        created_ms: 1,
    };
    let mut report:Report=serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":10,"requirements":[{"id":"new-contract","name":"New export condition","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Needs its own new check","evidence_citations":[0]}}]}})).unwrap();
    let mut a =
        json!({"passed":null,"needs_thinker":["new-contract"],"needs_user":[],"violations":[]});
    unchecked_final(&request, &report, &mut a).unwrap();
    assert_eq!(a["passed"], false);
    assert_eq!(a["needs_check"][0]["id"], "new-contract");
    assert!(a["needs_user"].as_array().unwrap().is_empty());
    a = json!({"passed":false,"state":"checkpoint_failed","needs_thinker":["new-contract"],"needs_user":[],"violations":["scope"]});
    unchecked_final(&request, &report, &mut a).unwrap();
    assert_eq!(a["state"], "checkpoint_failed");
    report.decision_units.as_mut().unwrap().version = Some(9);
    request.trigger = json!({});
    unchecked_final(&request, &report, &mut a).unwrap();
}
