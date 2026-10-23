use serde_json::{Value, json};

pub struct Case {
    pub name: &'static str,
    pub intent: Value,
    pub evidence: Value,
}
pub fn recorded() -> Value {
    serde_json::from_str(include_str!("../fixtures/completion-false-stop.json")).unwrap()
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
    }
}
