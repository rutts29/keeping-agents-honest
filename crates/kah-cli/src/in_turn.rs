//! Native post-tool events wake the observer only after a bounded tool batch.
use kah_observer::{
    engine,
    session::{Binding, Store},
    tool_watch::Progress,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, os::unix::fs::PermissionsExt, path::Path};

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
    let path = directory.join("tool-progress.json");
    let mut progress = read(&path)?;
    let id = input["tool_use_id"]
        .as_str()
        .ok_or("OBSERVER_TOOL_ID stage=in_turn")?;
    let turn = input["turn_id"]
        .as_str()
        .or_else(|| input["prompt_id"].as_str())
        .unwrap_or("session");
    let result = progress.push(turn, id, action(input, id))?;
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
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 || meta.len() > 128 * 1024 {
        return Err("OBSERVER_TOOL_PRIVATE stage=in_turn".into());
    }
    let file =
        fs::File::open(path).map_err(|e| format!("OBSERVER_TOOL_READ stage=in_turn: {e}"))?;
    let progress: Progress = serde_json::from_reader(file.take(128 * 1024 + 1))
        .map_err(|_| "OBSERVER_TOOL_JSON stage=in_turn")?;
    progress.validate()?;
    Ok(progress)
}
fn action(input: &Value, id: &str) -> Value {
    let mut facts = json!({"tool":input["tool_name"],"input":input["tool_input"],"response":input["tool_response"],"error":input["error"],"source":input["hook_event_name"],"scope":"A completed native tool call; input is context, not proof that a background command has finished."});
    crate::privacy::scrub(&mut facts);
    let text = facts.to_string();
    let mut end = text.len().min(4096);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    json!({"id":id,"kind":"toolResult","status":if input["hook_event_name"]=="PostToolUseFailure" {"failed"} else {"completed"},"evidence":&text[..end],"truncated":end<text.len()})
}
