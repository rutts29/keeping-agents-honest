//! Claude's documented native string limit is measured, never worked around.
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
    let text = report.reminder_text(&intent(request)?, &record.to_string_lossy())?;
    validate_native(&state.worker.provider, &text)?;
    Ok(text)
}
pub fn validate_native(provider: &str, text: &str) -> Result<()> {
    if provider == "claude" && text.encode_utf16().count() > 10_000 {
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
