use kah_observer::judge::{
    Questions,
    routing::{self, Backend},
};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "paired live Jev replay over actual native brain reports; make gate only"]
fn partial_phase_policy_replay() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    let cases = [
        (
            "codex-repaired",
            include_str!(
                "../../../docs/gates/evidence/phase-5o-c/failures/codex-after-tests-judgment.json"
            ),
            true,
        ),
        (
            "claude-working",
            include_str!(
                "../../../docs/gates/evidence/phase-5o-c/failures/claude-self-corrected-closed-deferral.json"
            ),
            true,
        ),
        (
            "genuine-future",
            include_str!(
                "../../../docs/gates/evidence/phase-5o-c/failures/claude-genuine-dependency.json"
            ),
            true,
        ),
        (
            "broken-existing",
            include_str!(
                "../../../docs/gates/evidence/phase-5o-c/failures/claude-review-1000000000003-1.json"
            ),
            false,
        ),
    ];
    let mut results = vec![];
    for (name, text, expected) in cases {
        results.push(case(&home, name, text, expected));
    }
    let passed = results.iter().all(|r| r["matchesExpected"] == true);
    fs::write(evidence.join("checkpoint-policy-v2-replay.json"),serde_json::to_vec_pretty(&json!({"cases":results,"checkpointQuestionSet":kah_observer::checkpoint::VERSION,"numericalThresholdsUnchanged":true,"nativeBrainReportsReused":true,"passed":passed})).unwrap()).unwrap();
    assert!(
        passed,
        "paired replay contains unproven cases; all outcomes retained"
    );
}
fn case(home: &std::path::Path, name: &str, text: &str, expected: bool) -> Value {
    let r: Value = serde_json::from_str(text).unwrap();
    let original: Value = serde_json::from_slice(
        &fs::read(r["assessment"]["private_replay"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let s = &original["request"]["state"];
    let state = kah_cli::observer_review::checkpoint::decision_context(
        &s["intent"],
        &s["event"]["trigger"],
        s["event"]["head"].as_str(),
        &s["event"]["actions"],
        s["event"]["open_and_proposed_deferrals"].clone(),
    );
    let mut questions: Questions = kah_observer::checkpoint::questions();
    for d in state["event"]["open_and_proposed_deferrals"]
        .as_array()
        .unwrap()
    {
        let id = d["id"].as_str().unwrap();
        questions.insert(
            format!("deferral.{id}"),
            kah_observer::checkpoint::deferral_question(id),
        );
    }
    let judgment = routing::evaluate(
        home,
        Backend::Jev,
        original["request"]["model"].as_str().unwrap(),
        &state,
        &questions,
    )
    .unwrap();
    let outcome = kah_cli::observer_review::checkpoint::assess(
        &judgment,
        serde_json::from_value(state["event"]["open_and_proposed_deferrals"].clone()).unwrap(),
    );
    json!({"case":name,"sourcePrivateCapture":r["assessment"]["private_replay"],"originalJudgment":r["judgment"],"model":original["request"]["model"],"questions":questions,"state":state,"judgment":judgment,"outcome":outcome,"expectedPass":expected,"matchesExpected":(outcome["passed"]==true)==expected,"authorityVerbatim":state["intent"]["goal"]==s["intent"]["goal"]&&state["intent"]["items"]==s["intent"]["items"],"selectedEvidenceUnchanged":state["event"]==s["event"]})
}
