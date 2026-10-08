//! Native app-server commandExecution and Claude tool-use/result linkage.
//! Official0.160 API reference; Claude2.1.292 native transcript fields and
//! official hook tool_use_id/tool_input documentation. No marker-text parsing.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
pub(super) fn contexts(provider: &str, rows: &[Value]) -> Vec<Value> {
    let mut commands: BTreeMap<String, Value> = BTreeMap::new();
    rows.iter()
        .map(|row| {
            if provider == "codex" {
                return codex(row);
            }
            claude(row, &mut commands)
        })
        .collect()
}
fn codex(row: &Value) -> Value {
    let item = &row["item"];
    if item["type"] != "commandExecution" {
        return json!([]);
    }
    let mut value = json!({"id":item["id"],"command":item["command"],"cwd":item["cwd"],
        "result":item["status"]!="inProgress" && !item["exitCode"].is_null()});
    value["result_digest"] = json!(digest(
        &json!({"status":item["status"],"exit_code":item["exitCode"],"output":item["aggregatedOutput"]})
    ));
    json!([value])
}
fn claude(row: &Value, commands: &mut BTreeMap<String, Value>) -> Value {
    let Some(parts) = row["message"]["content"].as_array() else {
        return json!([]);
    };
    let mut values = vec![];
    for part in parts {
        if row["type"] == "assistant" && part["type"] == "tool_use" && part["name"] == "Bash" {
            let Some(id) = part["id"].as_str() else {
                continue;
            };
            let value = json!({"id":id,"command":part["input"]["command"],"cwd":row["cwd"],"result":false,"result_digest":null});
            commands.insert(id.into(), value.clone());
            values.push(value);
        } else if row["type"] == "user" && part["type"] == "tool_result" {
            let Some(id) = part["tool_use_id"].as_str() else {
                continue;
            };
            if let Some(mut value) = commands.get(id).cloned() {
                value["result"] = json!(true);
                value["result_digest"] = json!(digest(
                    &json!({"native_result":row["toolUseResult"],"result":part})
                ));
                values.push(value);
            }
        }
    }
    json!(values)
}
fn digest(value: &Value) -> String {
    format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON value serializes"))
    )
}
