use super::*;
use crate::{context_budget::Calibration, judge};
use serde_json::{Value, json};
pub(super) fn plan(state: &State, input: &Value, questions: &judge::Questions) -> bool {
    state
        .judgments
        .iter()
        .rev()
        .find_map(|j| {
            j.metrics.input_utf8_bytes.map(|bytes| Calibration {
                input_utf8_bytes: bytes,
                input_tokens: j.metrics.input_tokens,
            })
        })
        .and_then(|c| c.plan(input, questions))
        .is_some_and(|p| p.prepare)
}
pub(super) fn stage(
    store: &mut Store,
    input: &Value,
    questions: &judge::Questions,
    purpose: &str,
    reason: &str,
) -> Result<(), String> {
    let packet = json!({"state":input,"questions":questions,"purpose":purpose,"reason":reason});
    store.append(&json!({"kind":"preparation_requested","source":"observer","packet":packet,"at_ms":crate::session::now_ms()}))?;
    Err("JEV_PREPARATION_PENDING stage=context_preparation".into())
}
pub(super) fn pending(state: &State) -> Result<Value, String> {
    use std::io::BufRead;
    let file = std::fs::File::open(
        Store::directory(
            &RunStore::default_state_root().map_err(|e| e.to_string())?,
            &state.binding.session,
        )
        .join("observations.jsonl"),
    )
    .map_err(|e| e.to_string())?;
    let mut latest = None;
    for line in std::io::BufReader::new(file).lines() {
        let row: Value =
            serde_json::from_str(&line.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if row["kind"] == "preparation_requested" {
            latest = Some(row["packet"].clone());
        }
    }
    latest.ok_or("JEV_PREPARATION_MISSING stage=context_preparation".into())
}
