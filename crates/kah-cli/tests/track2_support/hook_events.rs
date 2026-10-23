//! Current observer hooks replace the removed per-tool Jev path.
use serde_json::Value;
pub fn complete(observed: &[Value]) -> bool {
    ["SessionStart", "UserPromptSubmit", "Stop"]
        .iter()
        .all(|name| observed.iter().any(|row| row["event"] == *name))
}
