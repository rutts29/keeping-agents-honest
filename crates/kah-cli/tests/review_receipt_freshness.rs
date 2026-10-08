//! Native records and exact execution ids, never test-name heuristics.
use kah_cli::observer_review::{
    capture,
    state::{Purpose, Request},
    verdict_cache::receipts,
};
use kah_observer::{decision_units::Requirement, review_evidence::Citation};
use serde_json::{Value, json};
fn item(id: &str, command: &str, cwd: &str, output: &str) -> Value {
    json!({"item":{"type":"commandExecution","id":id,"command":command,"cwd":cwd,"status":"completed","exitCode":0,"aggregatedOutput":output}})
}
fn fixture(rows: Value) -> (Request, Requirement) {
    let doc = capture::native_document("codex", "worker", &rows).unwrap();
    let citations = ["cargo test parser", "OLD_PASS", "OLD_SHA"]
        .into_iter()
        .map(|quote| Citation {
            source: doc.id.clone(),
            digest: doc.digest.clone(),
            quote: quote.into(),
            occurrence: 0,
        })
        .collect::<Vec<_>>();
    let requirement = serde_json::from_value(json!({"id":"parser","name":"Parser","dimension":"integration","authority_items":["goal"],
        "citations":citations,"components":[],"checks":[{"name":"parser check","command_citation":0,"result_citation":1,"sha_citation":2,"sha":"a".repeat(40)}],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Native receipt","evidence_citations":[0,1,2]}})).unwrap();
    let request = Request {
        id: "review-receipts".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Parser"}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![doc],
        created_ms: 1,
    };
    (request, requirement)
}
#[test]
fn newer_same_command_invalidates_old_retained_receipt_but_unrelated_commands_do_not() {
    let old = item("old", "cargo test parser", "/repo", "OLD_PASS\nOLD_SHA");
    let (request, r) = fixture(json!([old]));
    let original = receipts::latest(&request, &r).unwrap();
    let (request, r) = fixture(json!([old, item("other", "git status", "/repo", "status")]));
    assert_eq!(original, receipts::latest(&request, &r).unwrap());
    let (request, r) = fixture(json!([
        old,
        item("new", "cargo test parser", "/repo", "FAIL")
    ]));
    assert!(
        receipts::latest(&request, &r)
            .unwrap_err()
            .to_string()
            .contains("older execution")
    );
}
#[test]
fn different_cwd_is_a_different_execution_and_missing_native_link_stays_unbound() {
    let old = item("old", "cargo test parser", "/repo", "OLD_PASS\nOLD_SHA");
    let (request, r) = fixture(json!([
        old,
        item("other", "cargo test parser", "/other", "FAIL")
    ]));
    assert_eq!(
        receipts::latest(&request, &r).unwrap()["parser check"]["id"],
        "old"
    );
    let mut missing = request;
    missing.documents[0].native_records[0].context["executions"] = json!([]);
    assert!(
        receipts::latest(&missing, &r)
            .unwrap_err()
            .to_string()
            .contains("REVIEW_CACHE_RECEIPT")
    );
}
#[test]
fn claude_new_tool_execution_invalidates_its_older_cited_result() {
    let old = json!([
        {"type":"assistant","uuid":"a1","cwd":"/repo","message":{"content":[{"type":"tool_use","name":"Bash","id":"t1","input":{"command":"cargo test parser"}}]}},
        {"type":"user","uuid":"r1","cwd":"/repo","toolUseResult":{"stdout":"OLD_PASS\nOLD_SHA"},"message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"OLD_PASS\nOLD_SHA"}]}}
    ]);
    let (mut request, r) = fixture(json!([item(
        "old",
        "cargo test parser",
        "/repo",
        "OLD_PASS\nOLD_SHA"
    )]));
    request.documents[0] = capture::native_document("claude", "worker", &old).unwrap();
    assert_eq!(
        receipts::latest(&request, &r).unwrap()["parser check"]["id"],
        "t1"
    );
    let mut changed = old;
    changed.as_array_mut().unwrap().extend([
        json!({"type":"assistant","uuid":"a2","cwd":"/repo","message":{"content":[{"type":"tool_use","name":"Bash","id":"t2","input":{"command":"cargo test parser"}}]}}),
        json!({"type":"user","uuid":"r2","cwd":"/repo","toolUseResult":{"stdout":"FAIL"},"message":{"content":[{"type":"tool_result","tool_use_id":"t2","content":"FAIL"}]}})
    ]);
    request.documents[0] = capture::native_document("claude", "worker", &changed).unwrap();
    assert!(
        receipts::latest(&request, &r)
            .unwrap_err()
            .to_string()
            .contains("older execution")
    );
}
