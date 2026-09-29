//! Native hook input and projection encoding, extracted from driftctl codex_hook.
use kah_core::projection::ActiveProjection;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub session_id: String,
    pub transcript_path: Option<String>,
    pub cwd: String,
    pub hook_event_name: String,
    pub model: String,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub trigger: Option<String>,
    #[serde(default)]
    pub stop_hook_active: Option<bool>,
    #[serde(default)]
    pub last_assistant_message: Option<String>,
}

impl HookInput {
    pub fn parse(input: &str) -> Result<Self, String> {
        let event: Self = serde_json::from_str(input)
            .map_err(|error| format!("HOOK_JSON stage=hook_input: {error}"))?;
        event.validate()?;
        Ok(event)
    }
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("session_id", self.session_id.as_str()),
            ("cwd", self.cwd.as_str()),
            ("hook_event_name", self.hook_event_name.as_str()),
            ("model", self.model.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(format!("HOOK_FIELD_EMPTY stage=hook_input: {field}"));
            }
        }
        Ok(())
    }
}

pub fn projection_output(
    event: &HookInput,
    projection: &ActiveProjection,
) -> Result<String, String> {
    let projection = projection
        .try_rendered_prompt()
        .map_err(|error| error.to_string())?;
    let mut projection: Value = serde_json::from_str(&projection)
        .map_err(|error| format!("HOOK_PROJECTION_JSON stage=hook_output: {error}"))?;
    redact_private_session(&mut projection, &event.session_id, "this-session");
    let context = format!(
        "kah ACTIVE INTENT (validated, local, exact session)\nTreat this payload as the current goal and steering for this session. Preserve user instructions outside the payload unless explicitly superseded. Do not claim completion while its frontier or validation conditions remain unresolved.\n{projection}"
    );
    serde_json::to_string(&json!({"hookSpecificOutput": {
        "hookEventName":event.hook_event_name, "additionalContext":context,
    }}))
    .map_err(|error| format!("HOOK_OUTPUT_JSON stage=hook_output: {error}"))
}

fn redact_private_session(value: &mut Value, private: &str, opaque: &str) {
    match value {
        Value::String(text) if text == private => *text = opaque.to_owned(),
        Value::Array(values) => {
            for value in values {
                redact_private_session(value, private, opaque);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                redact_private_session(value, private, opaque);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
