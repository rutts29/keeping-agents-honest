//! Native lifecycle evidence is separate from user-message authority.
use serde_json::Value;

pub fn completed(text: &str, session: &str) -> Result<bool, String> {
    if !text.is_empty() && !text.ends_with('\n') {
        return Ok(false);
    }
    let mut completed = false;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let row: Value =
            serde_json::from_str(line).map_err(|_| "CLAUDE_SOURCE_JSON stage=turn_completion")?;
        if row["isSidechain"] == true || row["sessionId"].as_str().is_some_and(|id| id != session) {
            continue;
        }
        match row["type"].as_str() {
            Some("user" | "assistant") => completed = false,
            Some("system") if row["subtype"] == "turn_duration" => completed = true,
            _ => (),
        }
    }
    Ok(completed)
}
