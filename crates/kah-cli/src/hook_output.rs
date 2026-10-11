//! Native hook metadata and lossless composition of context notices.
use serde_json::Value;

pub fn post_tool(input: &Value) -> bool {
    matches!(
        input["hook_event_name"].as_str(),
        Some("PostToolUse" | "PostToolUseFailure")
    )
}
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
