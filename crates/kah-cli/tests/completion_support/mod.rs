use serde_json::{Value, json};

pub struct Case {
    pub name: &'static str,
    pub intent: Value,
    pub evidence: Value,
}
const TASK: &str = "Create double.js exporting a function double(n) that returns n multiplied by two, and add an independent check for it.";
const HEAD: &str = "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d";

fn choice(satisfied: f64, confidence: f64) -> Value {
    json!({"type":"choice","choice":"satisfied","confidence":confidence,
        "probabilities":{"satisfied":satisfied,"unclear":0.02,"violated":1.0 - satisfied - 0.02}})
}
/// One typed judgment whose outcome answers clear the unattended thresholds.
fn judgment(satisfied: f64, confidence: f64) -> Value {
    json!({"backend":"jev","model":"synthetic-judge","fallback_reason":null,
        "answers":{
            "outcome_met.goal": choice(satisfied, confidence),
            "outcome_met.user-0001": choice(satisfied, confidence),
        },
        "metrics":{"calls":1,"failed_calls":0,"input_tokens":1000,"cached_input_tokens":0,
            "output_tokens":50,"elapsed_ms":100,"estimated_cost_usd":0.0001}})
}
/// A false-stop sample: the controller recorded `completed: false`, yet the
/// work is correct and both judgment channels qualify as complete.
pub fn false_stop_sample() -> Value {
    let tests = json!({"command":"node checks double.js","commit":HEAD,"elapsed_ms":120,
        "exit_code":0,"output":"CHECK_OK double(2) === 4","output_truncated":false,"success":true});
    let receipt = |first: Value, second: Value| {
        json!({"completed":false,"judgments":[first,second],
            "proof":{"commit":HEAD,"tests":tests,"unchanged":true,"userSequence":0}})
    };
    json!({"task":TASK,"receipts":[
        receipt(judgment(0.94, 0.91), judgment(0.90, 0.85)),
        receipt(judgment(0.92, 0.88), judgment(0.89, 0.83)),
    ]})
}
pub fn synthetic_case() -> Case {
    let data = false_stop_sample();
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
        "observed_activity":[{"id":"synthetic-edit","kind":"fileChange","status":"completed","evidence":{"path":"double.js"}},{"id":"synthetic-test","kind":"commandExecution","exit_code":0,"evidence":{"command":"node checks double.js"}}],
        "git_diff":"+function double(n) { return n * 2; }\n+module.exports = double;\n",
        "committed_files":[{"path":"double.js","complete":true,"content":"function double(n) { return n * 2; }\nmodule.exports = double;\n","source":"authored fixture"}],
        "tests":data["receipts"][0]["proof"]["tests"],
        "controller_facts":{"changed_paths":["double.js"],"changed_paths_complete":true,"git_diff_complete":true,"verification_left_candidate_unchanged":true},
        "enforced_policy":{"command_execution":"gVisor","workspace_online":false,"model_login":"host only","native_capabilities":"full","source_repository_writable_mount":false}
    });
    Case {
        name: "synthetic-code-task",
        intent,
        evidence,
    }
}
