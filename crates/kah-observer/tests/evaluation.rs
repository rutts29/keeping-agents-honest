use kah_observer::evaluation::dataset::{Row, prepare, validation_id};
use serde_json::json;
fn row(id: &str, session: &str, prior: serde_json::Value) -> Row {
    serde_json::from_value(json!({"id":id,"session":session,"ts":"now","source_kind":"user","user_text":"Exact steering","prev_assistant":"Prior agent context","prior_items":prior,"gold":{"is_steering":true,"steer_kind":"new_constraint","supersedes":"none","tags":[],"note":"fixture"}})).unwrap()
}
#[test]
fn gold_state_is_rebuilt_across_holdout_rows_without_sending_labels() {
    let a = row("s-0000", "s", json!([]));
    let b = row(
        "s-0003",
        "s",
        json!([{"id":"s-0000","text":"Exact steering"}]),
    );
    let cases = prepare(vec![a, b], false).unwrap();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].state["prior_intent"]["items"][0]["id"], "s-0000");
    assert!(cases[0].state.get("gold").is_none());
    assert_eq!(
        cases[0].state["event"]["previous_assistant"],
        "Prior agent context"
    );
    assert!(validation_id("example-0122"));
    assert!(!validation_id("example-0123"));
}
#[test]
fn inconsistent_gold_prior_and_duplicate_ids_are_rejected() {
    let a = row("s-0000", "s", json!([]));
    let b = row("s-0003", "s", json!([]));
    assert!(
        prepare(vec![a.clone(), b], false)
            .unwrap_err()
            .contains("GOLD_PRIOR_MISMATCH")
    );
    assert!(prepare(vec![a.clone(), a], false).is_err());
}

#[test]
#[ignore = "private gold-dataset consistency preflight; no model calls"]
fn real_dataset_gold_contexts_rebuild_before_any_evaluation() {
    let input =
        std::env::var_os("KAH_TEST_GOLD_DATASET").expect("explicit evaluation dataset required");
    let text = std::fs::read_to_string(input).unwrap();
    let rows = text
        .lines()
        .map(|l| serde_json::from_str::<Row>(l).unwrap())
        .collect::<Vec<_>>();
    let total = rows.len();
    let development = prepare(rows.clone(), false).unwrap();
    let validation = prepare(rows, true).unwrap();
    assert_eq!(development.len() + validation.len(), total);
    eprintln!(
        "development={} validation={} model_calls=0",
        development.len(),
        validation.len()
    );
    for case in development.iter().chain(&validation) {
        kah_observer::judge::validate_request(&case.state, &case.questions).unwrap();
    }
}
