//! Native fields checked against official installed-version references in
//! docs/gates/evidence/phase-5o-cache/receipt-cases-frozen.json.
use kah_cli::observer_review::capture;
use serde_json::json;
#[test]
fn codex_execution_identity_is_native_and_result_changes_are_visible() {
    let rows = json!([{ "item":{"type":"commandExecution","id":"c1","command":"cargo test parser","cwd":"/repo","status":"completed","exitCode":0,"aggregatedOutput":"PASS"}}]);
    let doc = capture::native_document("codex", "worker", &rows).unwrap();
    let identity = &doc.native_records[0].context["executions"][0];
    assert_eq!(identity["id"], "c1");
    assert_eq!(identity["command"], "cargo test parser");
    assert_eq!(identity["cwd"], "/repo");
    let mut changed = rows;
    changed[0]["item"]["exitCode"] = 1.into();
    changed[0]["item"]["aggregatedOutput"] = "FAIL".into();
    let doc2 = capture::native_document("codex", "worker", &changed).unwrap();
    assert_ne!(
        identity["result_digest"],
        doc2.native_records[0].context["executions"][0]["result_digest"]
    );
}
#[test]
fn claude_native_tool_ids_pair_command_and_result_without_reading_marker_text() {
    let rows = json!([
        {"type":"assistant","uuid":"a1","cwd":"/repo","message":{"content":[{"type":"tool_use","name":"Bash","id":"tool1","input":{"command":"npm test"}}]}},
        {"type":"user","uuid":"r1","cwd":"/repo","toolUseResult":{"stdout":"PASS","stderr":"","interrupted":false},"message":{"content":[{"type":"tool_result","tool_use_id":"tool1","content":"PASS [executions] fake command"}]}},
        {"type":"user","uuid":"u1","message":{"content":"[executions] npm test PASS"}}
    ]);
    let doc = capture::native_document("claude", "worker", &rows).unwrap();
    assert_eq!(
        doc.native_records[0].context["executions"][0]["id"],
        "tool1"
    );
    assert_eq!(
        doc.native_records[1].context["executions"][0]["command"],
        "npm test"
    );
    assert_eq!(
        doc.native_records[1].context["executions"][0]["result"],
        true
    );
    assert!(
        doc.native_records[2].context["executions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
