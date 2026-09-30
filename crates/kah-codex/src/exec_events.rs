//! Codex 0.159 `exec --json` events. Embedded tool output is always data.
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub enum Event {
    ThreadStarted(String),
    TurnStarted,
    Action(Value),
    Usage {
        input: u64,
        cached_input: u64,
        output: u64,
    },
    Failed(String),
    Notice(String),
    Progress,
}
pub fn decode(value: Value) -> Result<Event, String> {
    match value["type"].as_str() {
        Some("thread.started") => Ok(Event::ThreadStarted(required(&value, "thread_id")?.into())),
        Some("turn.started") => Ok(Event::TurnStarted),
        Some("turn.completed") => Ok(Event::Usage {
            input: counter(&value["usage"], "input_tokens")?,
            cached_input: value["usage"]["cached_input_tokens"].as_u64().unwrap_or(0),
            output: counter(&value["usage"], "output_tokens")?,
        }),
        Some("turn.failed") => Ok(Event::Failed(required(&value["error"], "message")?.into())),
        Some("error") => Ok(Event::Failed(required(&value, "message")?.into())),
        Some("item.started" | "item.updated") => Ok(Event::Progress),
        Some("item.completed") => action(&value["item"]),
        _ => Err("CODEX_EXEC_EVENT_TYPE stage=exec_intake: unsupported native event".into()),
    }
}
fn action(item: &Value) -> Result<Event, String> {
    let id = required(item, "id")?;
    let (kind, evidence) = match item["type"].as_str() {
        Some("agent_message") => (
            "agentMessage",
            json!({"text":bounded(required(item,"text")?,4096)}),
        ),
        Some("command_execution") => (
            "commandExecution",
            json!({
                "command":bounded(required(item,"command")?,1024),
                "output":bounded(item["aggregated_output"].as_str().unwrap_or(""),4096),
                "exit_code":item["exit_code"],"status":item["status"]
            }),
        ),
        Some("file_change") => (
            "fileChange",
            json!({"changes":item["changes"],"status":item["status"]}),
        ),
        Some("reasoning" | "todo_list") => return Ok(Event::Progress),
        Some("error") => return Ok(Event::Notice(required(item, "message")?.into())),
        Some("mcp_tool_call" | "web_search" | "collab_tool_call") => (
            "toolObservation",
            json!({"native_type":item["type"],"status":item["status"]}),
        ),
        _ => return Err("CODEX_EXEC_ITEM_TYPE stage=exec_intake: unsupported native item".into()),
    };
    if evidence.to_string().len() > 64 * 1024 {
        return Err("CODEX_EXEC_EVIDENCE_BOUND stage=exec_intake".into());
    }
    Ok(Event::Action(
        json!({"id":id,"kind":kind,"evidence":evidence,
        "exit_code":item["exit_code"],"status":item["status"]}),
    ))
}
fn counter(value: &Value, key: &str) -> Result<u64, String> {
    value[key]
        .as_u64()
        .ok_or_else(|| format!("CODEX_EXEC_USAGE stage=exec_intake: missing {key}"))
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("CODEX_EXEC_FIELD stage=exec_intake: missing {key}"))
}
fn bounded(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
