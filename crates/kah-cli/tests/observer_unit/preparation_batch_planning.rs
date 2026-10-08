use super::*;
use crate::observer_review::state::Purpose;
use serde_json::json;
#[test]
fn every_calibrated_over_budget_decision_is_staged_before_the_first_wake() {
    let worker = format!("batch-plan-{}", crate::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    // B preparation links whole originals through the bound observer workspace.
    crate::unattended_report::save(&store.directory,"state.json",&json!({
        "worker":{"provider":"codex","session":worker,"project":store.directory,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":store.directory,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"stopped":false})).unwrap();
    crate::unattended_report::save(
        &store.directory,
        "token-calibration.json",
        &json!({"input_utf8_bytes":100,"input_tokens":100}),
    )
    .unwrap();
    let input = json!({"intent":{"goal":"Whole goal","items":[]},"event":{"head":"sha","trigger":{"text":"whole evidence ".repeat(3000)}},"decision":{"proportion":{"reasoning":"whole reasoning ".repeat(3000)}}});
    let questions = Questions::from([
        (
            "claim_made".into(),
            kah_observer::judge::Question::Noul {
                instructions: "Check claim.".into(),
            },
        ),
        (
            "proportion".into(),
            kah_observer::judge::Question::Noul {
                instructions: "Check proportion.".into(),
            },
        ),
    ]);
    let request = Request {
        id: "review-plan".into(),
        worker: worker.clone(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: None,
        intent: input["intent"].clone(),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    assert!(plan(&request, &input, &questions).unwrap());
    let current: Value = state::read(&store.directory.join("preparation-current.json"))
        .unwrap()
        .unwrap();
    let batch = load(&worker, current["decision"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let staged = super::jobs(&worker, &batch).unwrap();
    assert_eq!(staged.len(), 2);
    for job in staged {
        assert_eq!(job.state["intent"]["goal"], "Whole goal");
        assert_eq!(job.questions.len(), 1, "one complete decision per job");
        assert!(
            !store
                .directory
                .join(format!("preparation-woken-{}.json", job.decision))
                .exists()
        );
    }
    let directory = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(directory).unwrap();
}
