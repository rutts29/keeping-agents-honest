use super::*;

pub fn bind_correction(store: &Store, request: &str, number: u64, gap: &Value) -> Result<()> {
    let mut log = load(store)?;
    let attempt = log["requests"][request]["attempts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["number"] == number)
        .ok_or("REVIEW_REDELIVERY_ATTEMPT stage=telemetry")?;
    attempt["report_digest"] = gap["report_digest"].clone();
    attempt["request_digest"] = gap["request_digest"].clone();
    crate::unattended_report::save(&store.directory, "redelivery.json", &log)
}
pub fn unchanged_correction(store: &Store, request: &str, gap: &Value) -> Result<bool> {
    if gap["error"].as_str().is_some_and(|error| {
        error.starts_with("REVIEW_SOURCE_OUTSIDE_PACKET")
            || error.starts_with("REVIEW_EXPANSION_PENDING")
    }) {
        // Host publication is not a thinker correction. Retained retries back
        // off independently; an unchanged observer report cannot repair it.
        return Ok(false);
    }
    if gap["report_digest"].is_null() || gap["request_digest"].is_null() {
        return Ok(false);
    }
    let log = load(store)?;
    Ok(log["requests"][request]["attempts"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|a| {
            a["status"] == "submitted_waiting_for_report"
                && a["prior_error"] == gap["error"]
                && a["report_digest"] == gap["report_digest"]
                && a["request_digest"] == gap["request_digest"]
        }))
}
pub fn park_correction(store: &Store, request: &str, gap: &Value) -> Result<()> {
    super::parked::release(store, request, gap)?;
    let mut blocked = gap.clone();
    blocked["status"] = "validation_blocked_unchanged".into();
    blocked["request"] = request.into();
    let prior: Option<Value> =
        super::super::state::read(&store.directory.join("validation-blocked.json"))?;
    if prior.as_ref().is_some_and(|p| {
        p["request"] == request
            && p["error"] == gap["error"]
            && p["report_digest"] == gap["report_digest"]
            && p["request_digest"] == gap["request_digest"]
    }) {
        return Ok(());
    }
    let text = format!(
        "kah observer request {request}: unchanged validation failure after correction prompt: {}. Evidence retained; no re-ask until report or request changes.",
        gap["error"]
    );
    blocked["text"] = text.clone().into();
    crate::unattended_report::save(&store.directory, "validation-blocked.json", &blocked)?;
    crate::unattended_report::save(
        &store.directory,
        "redelivery-notice.json",
        &serde_json::json!({"request":request,"text":text}),
    )
}

pub fn begin_correction(store: &Store, request: &str, gap: &Value) -> Result<u64> {
    let number = begin(
        store,
        request,
        gap["error"].as_str().unwrap_or("missing evidence report"),
    )?;
    bind_correction(store, request, number, gap)?;
    Ok(number)
}
