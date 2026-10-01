//! Two evidence views of the same verified candidate, without changing intent.
use serde_json::{Value, json};

pub fn states(intent: &Value, evidence: &Value) -> Result<[Value; 2], String> {
    if !evidence["committed_files"].is_array()
        || !evidence["git_diff"].is_string()
        || !evidence["tests"].is_object()
        || !evidence["controller_facts"].is_object()
        || !evidence["enforced_policy"].is_object()
    {
        return Err("COMPLETION_EVIDENCE_SHAPE stage=completion_inputs".into());
    }
    let tests = &evidence["tests"];
    let artifact = json!({
        "candidate_commit":tests["commit"],
        "committed_files":evidence["committed_files"],
        "git_diff":evidence["git_diff"],
        "tests":{"commit":tests["commit"],"command":tests["command"],"success":tests["success"],"exit_code":tests["exit_code"],"output":tests["output"],"output_truncated":tests["output_truncated"]},
        "controller_facts":evidence["controller_facts"],
        "enforced_policy":evidence["enforced_policy"]
    });
    Ok([
        json!({"intent":intent,"event":{"actions":evidence}}),
        json!({"intent":intent,"event":{"actions":artifact}}),
    ])
}
