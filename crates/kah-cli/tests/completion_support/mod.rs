use kah_cli::{
    completion_capture::{self, Request},
    completion_inputs,
};
use kah_core::projection::ActiveProjection;
use kah_observer::judge;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Case {
    pub name: &'static str,
    pub intent: Value,
    pub evidence: Value,
    pub outcomes: Vec<(String, String)>,
}
pub fn recorded() -> Value {
    serde_json::from_str(include_str!("../fixtures/completion-false-stop.json")).unwrap()
}
pub fn original(home: &Path) -> Case {
    let data: Value = serde_json::from_slice(
        &fs::read(
            home.join(".local/share/kah-eval/completion-fixtures/infrastructure-original.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let projection: ActiveProjection = serde_json::from_value(data["projection"].clone()).unwrap();
    let mut outcomes = vec![("goal".into(), projection.goal.text.clone())];
    outcomes.extend(
        projection
            .preserve
            .iter()
            .chain(&projection.frontier)
            .chain(&projection.validation)
            .map(|i| (i.id.to_string(), i.text.clone())),
    );
    let intent = json!({"goal":projection.goal.text,"outcomes":outcomes,"user_confirmed_ids":projection.frontier.iter().filter(|i|i.user_confirmed).map(|i|i.id.to_string()).collect::<Vec<_>>()});
    Case {
        name: "intact-infrastructure",
        intent,
        evidence: data["evidence"].clone(),
        outcomes,
    }
}
pub fn reconstructed() -> Case {
    let data = recorded();
    let task = data["task"].as_str().unwrap();
    let outcomes = data["receipts"][0]["judgments"][0]["answers"]
        .as_object()
        .unwrap()
        .keys()
        .map(|key| {
            (
                key.strip_prefix("outcome_met.").unwrap().to_owned(),
                task.to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let intent = json!({"goal":task,"outcomes":outcomes,"user_confirmed_ids":[]});
    let evidence = json!({
        "evaluation_scope":"The candidate code and its independent tests.",
        "observed_activity":[{"id":"reconstructed-edit","kind":"fileChange","status":"completed","evidence":{"path":"double.js"}},{"id":"reconstructed-test","kind":"commandExecution","exit_code":0,"evidence":{"command":"node checks double.js"}}],
        "git_diff":"+function double(n) { return n * 2; }\n+module.exports = double;\n",
        "committed_files":[{"path":"double.js","complete":true,"content":"function double(n) { return n * 2; }\nmodule.exports = double;\n","source":"reconstructed fixture, not recovered original code"}],
        "tests":data["receipts"][0]["proof"]["tests"],
        "controller_facts":{"changed_paths":["double.js"],"changed_paths_complete":true,"git_diff_complete":true,"verification_left_candidate_unchanged":true},
        "enforced_policy":{"command_execution":"gVisor","workspace_online":false,"model_login":"host only","native_capabilities":"full","source_repository_writable_mount":false}
    });
    Case {
        name: "reconstructed-code-task",
        intent,
        evidence,
        outcomes,
    }
}
pub fn compare(home: &Path, case: &Case) -> Value {
    let states = completion_inputs::states(&case.intent, &case.evidence).unwrap();
    let questions = judge::questions::completion(&case.outcomes);
    let mut results = vec![];
    for (channel, state) in [
        ("shared-first", &states[0]),
        ("old-second", &states[0]),
        ("new-second", &states[1]),
    ] {
        let request = Request::new("jev-latest", state, &questions).unwrap();
        let result = judge::routing::evaluate(
            home,
            judge::routing::Backend::Jev,
            request.model(),
            request.state(),
            request.questions(),
        );
        let saved = completion_capture::save(
            home,
            case.name,
            case.evidence["tests"]["commit"].as_str().unwrap(),
            channel,
            &request,
            &result,
        )
        .unwrap();
        let judgment = result.unwrap();
        results.push(json!({"channel":channel,"requestReplay":saved,"complete":judge::decision::complete(&judgment,&case.outcomes),"judgment":judgment}));
    }
    json!({"case":case.name,"originalStateRetained":case.name=="intact-infrastructure","reconstructed":case.name=="reconstructed-code-task","oldComplete":results[0]["complete"]==true&&results[1]["complete"]==true,"newComplete":results[0]["complete"]==true&&results[2]["complete"]==true,"results":results,"questionsAndThresholdsUnchanged":true})
}
pub fn evidence() -> PathBuf {
    PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap())
}
