//! Recognize version-matched native search records, never an agent's claim.
use serde_json::Value;
pub fn payload(row: &Value) -> &Value {
    if row["type"] == "response_item" {
        &row["payload"]
    } else {
        row
    }
}
pub fn calls(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(payload)
        .filter(|r| {
            r["type"] == "custom_tool_call"
                && r["name"] == "exec"
                && r["input"]
                    .as_str()
                    .is_some_and(|s| s.contains("tools.web__run"))
                || r["type"] == "function_call"
                    && (matches!(r["name"].as_str(), Some("web.run" | "webrun"))
                        || r["name"] == "run" && r["namespace"] == "web")
        })
        .cloned()
        .collect()
}
pub fn results(rows: &[Value], calls: &[Value]) -> Vec<Value> {
    rows.iter()
        .map(payload)
        .filter(|r| {
            matches!(
                r["type"].as_str(),
                Some("function_call_output" | "custom_tool_call_output")
            ) && calls.iter().any(|c| c["call_id"] == r["call_id"])
        })
        .cloned()
        .collect()
}
pub fn verify(rows: &[Value]) -> (Vec<Value>, Vec<Value>) {
    let calls = calls(rows);
    let results = results(rows, &calls);
    assert!(
        !calls.is_empty() && !results.is_empty(),
        "native search call/result missing"
    );
    assert!(
        results
            .iter()
            .any(|r| r["output"].to_string().contains("iana.org")),
        "native search returned no IANA result"
    );
    (calls, results)
}
