mod completion_support;
use kah_observer::judge;
use serde_json::json;
use std::{fs, path::PathBuf};

#[test]
fn recorded_false_stops_complete_at_the_provisional_completion_bar() {
    let data = completion_support::recorded();
    let task = data["task"].as_str().unwrap();
    for receipt in data["receipts"].as_array().unwrap() {
        assert_eq!(receipt["completed"], false);
        let mut passed = 0;
        for value in receipt["judgments"].as_array().unwrap() {
            let judgment: judge::Judgment = serde_json::from_value(value.clone()).unwrap();
            let outcomes = judgment
                .answers
                .keys()
                .map(|key| {
                    (
                        key.strip_prefix("outcome_met.").unwrap().to_owned(),
                        task.to_owned(),
                    )
                })
                .collect::<Vec<_>>();
            passed += usize::from(judge::decision::complete(&judgment, &outcomes));
        }
        // Both channels of each recorded false stop (correct work) now pass.
        assert_eq!(passed, 2);
    }
    assert_eq!(judge::questions::UNATTENDED_COMPLETE_PROBABILITY, 0.85);
    assert_eq!(judge::questions::UNATTENDED_COMPLETE_CONFIDENCE, 0.8);
    assert_eq!(judge::questions::COMPLETE_PROBABILITY, 0.9);
    assert_eq!(judge::questions::COMPLETE_CONFIDENCE, 0.85);
}

#[test]
#[ignore = "live Jev replay of intact and explicitly reconstructed completion cases"]
fn compare_completion_constructions() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let results = [
        completion_support::original(&home),
        completion_support::reconstructed(),
    ]
    .iter()
    .map(|case| completion_support::compare(&home, case))
    .collect::<Vec<_>>();
    fs::write(completion_support::evidence().join("replay.json"),serde_json::to_vec_pretty(&json!({"passed":true,"nativeAgentCalls":0,"original083084RequestAvailable":false,"recordedDecisionReplay":"completes at the provisional unattended completion bar","cases":results})).unwrap()).unwrap();
}

#[test]
fn the_labeled_reconstruction_has_both_completion_views() {
    let case = completion_support::reconstructed();
    assert_eq!(case.name, "reconstructed-code-task");
    assert_eq!(case.evidence["tests"]["success"], true);
    kah_cli::completion_inputs::states(&case.intent, &case.evidence).unwrap();
}

#[test]
#[ignore = "aggregate the five fixed paired live trials per provider"]
fn summarize_completion_trials() {
    let mut providers = vec![];
    // The demos set KAH_GATE_AGENTS (default both); a Claude-only run summarizes Claude only.
    let selected = std::env::var("KAH_GATE_AGENTS").unwrap_or_else(|_| "claude codex".into());
    let agents = selected.split_whitespace().collect::<Vec<_>>();
    for agent in agents.iter() {
        let rows = (1..=5)
            .map(|trial| {
                let path = completion_support::evidence()
                    .join(format!("native-completion-{trial}-{agent}.json"));
                serde_json::from_slice::<serde_json::Value>(&fs::read(path).unwrap()).unwrap()
            })
            .collect::<Vec<_>>();
        assert!(
            rows.iter()
                .all(|row| row["passed"] == true && row["independentTestsPassed"] == true)
        );
        providers.push(json!({"provider":agent,"trials":rows.len(),"oldFalseStops":rows.iter().filter(|r|r["oldFalseStop"]==true).count(),"newFalseStops":rows.iter().filter(|r|r["newFalseStop"]==true).count(),"actualControllerStops":rows.iter().filter(|r|r["controllerStatus"]=="stopped").count(),"versions":rows.iter().map(|r|r["version"].clone()).collect::<Vec<_>>(),"fixtureModels":rows.iter().map(|r|r["fixtureModel"].clone()).collect::<Vec<_>>() }));
    }
    fs::write(completion_support::evidence().join("paired-summary.json"),serde_json::to_vec_pretty(&json!({"passed":true,"agents":agents,"providers":providers,"pairedDesign":"same verified candidate and shared first judgment; old repeats the activity view, new uses the artifact view","thresholdsChanged":false,"limitations":"Five selected simple tasks per provider, not a general accuracy estimate. Original 0.83/0.84 request state was not available."})).unwrap()).unwrap();
}
