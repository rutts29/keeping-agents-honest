use kah_core::projection::ActiveProjection;
use kah_observer::judge;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "replay a captured fixture's completion evidence; no native agent call"]
fn compare_equivalent_evidence_encodings() {
    let path = PathBuf::from(
        std::env::var_os("KAH_COMPLETION_REPLAY").expect("fixture replay path required"),
    );
    let captured: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let projection: ActiveProjection =
        serde_json::from_value(captured["projection"].clone()).unwrap();
    assert!(
        projection
            .goal
            .text
            .starts_with("In this disposable worktree,")
    );
    let mut outcomes = vec![("goal".into(), projection.goal.text.clone())];
    outcomes.extend(
        projection
            .preserve
            .iter()
            .chain(&projection.frontier)
            .chain(&projection.validation)
            .map(|item| (item.id.to_string(), item.text.clone())),
    );
    let questions = judge::questions::completion(&outcomes);
    let original = json!({"intent":{"goal":projection.goal.text,"outcomes":outcomes,"user_confirmed_ids":projection.frontier.iter().filter(|i|i.user_confirmed).map(|i|i.id.to_string()).collect::<Vec<_>>()},"event":{"actions":captured["evidence"]}});
    let mut structured = original.clone();
    for action in structured["event"]["actions"]["observed_activity"]
        .as_array_mut()
        .unwrap()
    {
        if action["truncated"] != true
            && let Some(text) = action["evidence"].as_str()
            && let Ok(value) = serde_json::from_str::<Value>(text)
        {
            action["evidence"] = value;
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let mut results = vec![];
    for (encoding, state) in [("original", original), ("structured", structured)] {
        for _ in 0..2 {
            let result = judge::routing::evaluate(
                &home,
                judge::routing::Backend::Jev,
                captured["jevModel"].as_str().unwrap(),
                &state,
                &questions,
            )
            .unwrap();
            results.push(json!({"encoding":encoding,"complete":judge::decision::complete(&result,&outcomes),"judgment":result}));
        }
    }
    eprintln!("{}", serde_json::to_string_pretty(&results).unwrap());
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(evidence.join("encoding-comparison.json"),serde_json::to_vec_pretty(&json!({"sameGoalAndFacts":true,"sameQuestionsAndThresholds":true,"nativeAgentCalls":0,"results":results})).unwrap()).unwrap();
}
