//! Queued reminders reach the worker through native hooks. A Claude string
//! over its native limit goes in ordered parts: Claude would save it to a file
//! that kah's worker permissions deny. The native limit check
//! also stays for messages typed into a pane.
use super::state::{Request, State, Store};
use crate::Result;
use kah_observer::review_evidence::Report;
use serde_json::json;

pub fn save(store: &Store, request: &Request, report: &Report) -> Result<()> {
    let state = store.load()?.ok_or("REVIEW_STATE_MISSING stage=reminder")?;
    let text = text(&state, request, report)?;
    super::reminder_queue::save(
        &store.directory,
        &json!({"request":request.id,"turn":request.turn,"purpose":request.purpose,"text":text,"source":"observer_composed"}),
    )
}
pub fn text(state: &State, request: &Request, report: &Report) -> Result<String> {
    let record = state
        .observer
        .workspace
        .join("inputs")
        .join(&request.id)
        .join("intent.json");
    Ok(report.reminder_text(&intent(request)?, &record.to_string_lossy())?)
}
/// Claude's native limit for one hook string, in UTF-16 units.
pub const CLAUDE_LIMIT: usize = 10_000;
/// Room in each part for its marker.
const MARKER_UNITS: usize = 64;
/// The next hook string of a reminder, and the text left for later hooks.
/// `parts` counts the parts already delivered. Every character is delivered
/// in order; this is not a cap. Codex and short Claude text go whole.
pub fn next_part(provider: &str, text: &str, parts: u64) -> (String, Option<String>) {
    if provider != "claude" || parts == 0 && text.encode_utf16().count() <= CLAUDE_LIMIT {
        return (text.into(), None);
    }
    let (body, rest) = split(text);
    let current = parts + 1;
    let (mut total, mut left) = (current, rest);
    while !left.is_empty() {
        left = split(left).1;
        total += 1;
    }
    if rest.is_empty() {
        return (
            format!("[kah message part {current}/{total}]\n{body}"),
            None,
        );
    }
    let marker = format!("[kah message part {current}/{total}; continues at your next step]");
    (format!("{marker}\n{body}"), Some(rest.into()))
}
/// The longest start that fits one part, ending at a line end when it has one,
/// else at a character boundary.
fn split(text: &str) -> (&str, &str) {
    let budget = CLAUDE_LIMIT - MARKER_UNITS;
    let (mut units, mut line) = (0, None);
    for (index, c) in text.char_indices() {
        units += c.len_utf16();
        if units > budget {
            let end = line.unwrap_or(index);
            return text.split_at(end);
        }
        if c == '\n' {
            line = Some(index + 1);
        }
    }
    (text, "")
}
pub fn validate_native(provider: &str, text: &str) -> Result<()> {
    if provider == "claude" && text.encode_utf16().count() > CLAUDE_LIMIT {
        return Err("REVIEW_REMINDER_NATIVE_LIMIT stage=reminder: Claude hook string exceeds its documented 10000 UTF-16 units; original retained, concise observer reminder required".into());
    }
    Ok(())
}
pub fn validate_report(report: &Report, request: &Request) -> Result<()> {
    if report.reminder.is_some() {
        report.reminder_text(&intent(request)?, "full intent record")?;
    }
    Ok(())
}
pub fn intent(request: &Request) -> Result<serde_json::Value> {
    let mut intent = request.intent.clone();
    if let Some(value) = request.trigger.get("uncertain_user_input") {
        let input: kah_observer::intake::Input = serde_json::from_value(value.clone())?;
        if input.source.role != kah_core::intent_history::SourceRole::User {
            return Err("REVIEW_REMINDER_AUTHORITY stage=reminder".into());
        }
        intent["items"]
            .as_array_mut()
            .ok_or("REVIEW_REMINDER_INTENT stage=reminder")?
            .push(json!({"id":input.id(),"text":input.text,"status":"pending_verification"}));
    }
    Ok(intent)
}
