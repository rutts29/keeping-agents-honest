//! Parent completion requires a correlated native child result, not file creation.
use serde_json::Value;

pub fn completed(rows: &[Value]) -> bool {
    let calls = rows
        .iter()
        .flat_map(content)
        .filter(|c| c["type"] == "tool_use" && matches!(c["name"].as_str(), Some("Agent" | "Task")))
        .filter_map(|c| c["id"].as_str())
        .collect::<Vec<_>>();
    let mut agents = vec![];
    let mut result = None;
    for (index, row) in rows.iter().enumerate() {
        if row["type"] != "user" {
            continue;
        }
        let related = content(row).any(|c| {
            c["type"] == "tool_result"
                && calls.iter().any(|id| c["tool_use_id"] == *id)
                && c["is_error"] != true
        });
        if related {
            if row["toolUseResult"]["isAsync"] == true
                || row["toolUseResult"]["status"] == "async_launched"
            {
                if let Some(id) = row["toolUseResult"]["agentId"].as_str() {
                    agents.push(id);
                }
            } else {
                result = Some(index);
            }
        }
        // Installed 2.1.289 emits background results as native user-role
        // task-notification XML; task-id is the returned agentId.
        if texts(row).any(|s| {
            s.contains("<task-notification>")
                && s.contains("<status>completed</status>")
                && agents
                    .iter()
                    .any(|id| s.contains(&format!("<task-id>{id}</task-id>")))
        }) {
            result = Some(index);
        }
    }
    let Some(result) = result else { return false };
    let Some(done) = rows.iter().enumerate().skip(result + 1).find_map(|(i, r)| {
        (r["type"] == "assistant" && texts(r).any(|s| s.contains("CHILD_DONE"))).then_some(i)
    }) else {
        return false;
    };
    rows[done + 1..]
        .iter()
        .any(|r| r["type"] == "system" && r["subtype"] == "turn_duration")
}

fn content(row: &Value) -> impl Iterator<Item = &Value> {
    row["message"]["content"].as_array().into_iter().flatten()
}

fn texts(row: &Value) -> impl Iterator<Item = &str> {
    row["message"]["content"].as_str().into_iter().chain(
        content(row)
            .filter(|c| c["type"] == "text")
            .filter_map(|c| c["text"].as_str()),
    )
}

#[test]
fn background_parent_turn_does_not_mean_child_finished() {
    use serde_json::json;
    let mut rows = vec![
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","id":"call"}]}}),
        json!({"type":"user","toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"child"},"message":{"content":[{"type":"tool_result","tool_use_id":"call","content":"launched"}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"text","text":"CHILD_DONE"}]}}),
        json!({"type":"system","subtype":"turn_duration"}),
    ];
    assert!(!completed(&rows));
    rows.push(json!({"type":"user","isMeta":true,"message":{"content":[{"type":"text","text":"<task-notification><task-id>wrong</task-id><status>completed</status></task-notification>"}]}}));
    assert!(!completed(&rows));
    rows.push(json!({"type":"user","isMeta":true,"message":{"content":"<task-notification><task-id>child</task-id><status>completed</status></task-notification>"}}));
    assert!(!completed(&rows));
    rows.push(
        json!({"type":"assistant","message":{"content":[{"type":"text","text":"CHILD_DONE"}]}}),
    );
    assert!(!completed(&rows));
    rows.push(json!({"type":"system","subtype":"turn_duration"}));
    assert!(completed(&rows));
}

#[test]
fn foreground_result_also_needs_later_parent_reply_and_completion() {
    use serde_json::json;
    let rows = vec![
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"Agent","id":"call"}]}}),
        json!({"type":"user","toolUseResult":{"status":"completed"},"message":{"content":[{"type":"tool_result","tool_use_id":"call","content":"done"}]}}),
        json!({"type":"assistant","message":{"content":[{"type":"text","text":"CHILD_DONE"}]}}),
        json!({"type":"system","subtype":"turn_duration"}),
    ];
    assert!(completed(&rows));
}
