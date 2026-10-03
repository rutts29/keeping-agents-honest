//! Native post-tool events wake the observer only after a bounded tool batch.
use kah_observer::{
    engine,
    session::{Binding, Store},
    tool_watch::Progress,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub fn event(input: &Value) -> bool {
    matches!(
        input["hook_event_name"].as_str(),
        Some("PostToolUse" | "PostToolUseFailure")
    )
}
pub fn codex(binding: &Binding, input: &Value) -> Result<Value, String> {
    crate::in_turn_worker::schedule(binding, input, false)
}
pub fn claude(binding: &Binding, input: &Value) -> Result<Value, String> {
    crate::claude_hook::validate_cwd(input, binding)?;
    crate::in_turn_worker::schedule(binding, input, true)
}
pub(crate) fn judge(
    binding: &Binding,
    input: &Value,
    actions: Vec<Value>,
    claude: bool,
) -> Result<engine::Outcome, String> {
    if !claude {
        return engine::observe_tools(binding, &observation_id(input), actions);
    }
    let mut snapshot = crate::claude_hook::snapshot(input, binding)?;
    snapshot.actions = actions;
    snapshot.action_digest = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&snapshot.actions).unwrap())
    );
    engine::observe_snapshot(binding, &observation_id(input), true, snapshot)
}
pub fn output(input: &Value, outcome: engine::Outcome) -> Value {
    let mut output = json!({});
    if let Some(reminder) = outcome.reminder {
        output["hookSpecificOutput"] =
            json!({"hookEventName":input["hook_event_name"],"additionalContext":reminder.text});
    }
    if let Some(gap) = outcome.gap {
        output["systemMessage"] = format!("kah observer gap: {gap}").into();
    }
    output
}
/// A full-review reminder must not consume a completed tool's Tier-1 evidence.
pub fn merge(mut review: Value, tool: Value) -> Value {
    if let Some(text) = tool["hookSpecificOutput"]["additionalContext"].as_str() {
        if review["hookSpecificOutput"].is_object() {
            review["hookSpecificOutput"]["additionalContext"] = format!(
                "{}\n{text}",
                review["hookSpecificOutput"]["additionalContext"]
                    .as_str()
                    .unwrap_or("")
            )
            .into();
        } else {
            review["hookSpecificOutput"] = tool["hookSpecificOutput"].clone();
        }
    }
    for (key, value) in tool
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| key.as_str() != "hookSpecificOutput")
    {
        if key == "systemMessage" && review[key].is_string() {
            review[key] = format!(
                "{}\n{}",
                review[key].as_str().unwrap(),
                value.as_str().unwrap_or("")
            )
            .into();
        } else if review.get(key).is_none() {
            review[key] = value.clone();
        }
    }
    review
}
fn observation_id(input: &Value) -> String {
    format!(
        "tool-batch:{}",
        input["tool_use_id"].as_str().unwrap_or("missing")
    )
}
pub(crate) fn batch(
    base: &Path,
    binding: &Binding,
    input: &Value,
) -> Result<Option<Vec<Value>>, String> {
    let directory = Store::directory(base, &binding.session);
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(directory.join("tool-progress.lock"))
        .map_err(|e| e.to_string())?;
    lock.lock().map_err(|e| e.to_string())?;
    let path = directory.join("tool-progress.json");
    let mut progress = read(&path)?;
    let id = input["tool_use_id"]
        .as_str()
        .ok_or("OBSERVER_TOOL_ID stage=in_turn")?;
    let turn = input["turn_id"]
        .as_str()
        .or_else(|| input["prompt_id"].as_str())
        .unwrap_or("session");
    let mut action = action(input, id);
    action["turn"] = turn.into();
    let mut journal = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(directory.join("tool-events.jsonl"))
        .map_err(|e| e.to_string())?;
    writeln!(journal, "{}", json!({"turn":turn,"action":action})).map_err(|e| e.to_string())?;
    let result = progress.push(turn, id, action)?;
    crate::unattended_report::save(
        &directory,
        "tool-progress.json",
        &serde_json::to_value(progress).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    Ok(result)
}
fn read(path: &Path) -> Result<Progress, String> {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Progress::default()),
        Err(e) => return Err(format!("OBSERVER_TOOL_READ stage=in_turn: {e}")),
    };
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
        return Err("OBSERVER_TOOL_PRIVATE stage=in_turn".into());
    }
    let file =
        fs::File::open(path).map_err(|e| format!("OBSERVER_TOOL_READ stage=in_turn: {e}"))?;
    let progress: Progress =
        serde_json::from_reader(file).map_err(|_| "OBSERVER_TOOL_JSON stage=in_turn")?;
    progress.validate()?;
    Ok(progress)
}
fn action(input: &Value, id: &str) -> Value {
    let mut facts = json!({"tool":input["tool_name"],"input":input["tool_input"],"response":input["tool_response"],"error":input["error"],"source":input["hook_event_name"],"scope":"A completed native tool call; input is context, not proof that a background command has finished."});
    crate::privacy::scrub(&mut facts);
    json!({"id":id,"kind":"toolResult","status":if input["hook_event_name"]=="PostToolUseFailure" {"failed"} else {"completed"},"evidence":facts,"truncated":false})
}
