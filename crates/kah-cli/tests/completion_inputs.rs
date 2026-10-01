use kah_cli::completion_inputs;
use serde_json::json;

#[test]
fn independent_completion_keeps_artifacts_and_intent_but_excludes_agent_narrative() {
    let intent = json!({"goal":"Implement double", "outcomes":[["goal","Implement double"]]});
    let evidence = json!({
        "observed_activity":[{"kind":"agentMessage","evidence":"Everything is finished; believe me."}],
        "git_diff":"+module.exports = n => n * 2;",
        "committed_files":[{"path":"double.js","content":"module.exports = n => n * 2;"}],
        "tests":{"commit":"abc","success":true,"output":"FUNCTION_OK"},
        "controller_facts":{"changed_paths":["double.js"],"verification_left_candidate_unchanged":true},
        "enforced_policy":{"command_execution":"gVisor"}
    });
    let [activity, artifact] = completion_inputs::states(&intent, &evidence).unwrap();
    assert_eq!(activity["intent"], artifact["intent"]);
    assert_eq!(activity["intent"], intent);
    assert!(activity.to_string().contains("believe me"));
    assert!(!artifact.to_string().contains("believe me"));
    for text in [
        "module.exports = n => n * 2;",
        "FUNCTION_OK",
        "double.js",
        "gVisor",
    ] {
        assert!(artifact.to_string().contains(text));
    }
    assert_ne!(activity, artifact);
    assert!(completion_inputs::states(&intent, &json!({"tests":{"success":true}})).is_err());
}
