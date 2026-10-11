//! An unchanged provider-rejected context is an observation, never another
//! automatic rethink, a content cap, or a fabricated Jev decision.
use super::*;
pub fn known(request: &Request, prepared: &Value, questions: &Questions) -> Result<bool> {
    let path = Store::directory(&request.worker)?
        .join("context-rejections")
        .join(format!("{}.json", request.id));
    if !path.exists() {
        return Ok(false);
    }
    use super::super::source_store::{Select, read_selected};
    let record = read_selected(
        &path,
        &Select::fields(&[
            ("state", Select::All),
            ("prepared_digest", Select::All),
            ("questions", Select::All),
        ]),
    )?;
    Ok(record["state"] == "unconfirmed_context"
        && record["prepared_digest"] == context_selection::digest(prepared)
        && record["questions"] == serde_json::to_value(questions)?)
}
pub fn record(
    request: &Request,
    job: &Job,
    manifest: &Manifest,
    prepared: &Value,
    error: &str,
) -> Result<bool> {
    let root = Store::directory(&request.worker)?.join("context-rejections");
    kah_core::run_store::ensure_private_directory(&root)?;
    let name = format!("{}.json", request.id);
    let digest = context_selection::digest(prepared);
    let questions = serde_json::to_value(&job.questions)?;
    use super::super::source_store::{Select, read_selected};
    let previous = if root.join(&name).exists() {
        Some(read_selected(
            &root.join(&name),
            &Select::fields(&[
                ("prepared_digest", Select::All),
                ("error", Select::All),
                ("questions", Select::All),
            ]),
        )?)
    } else {
        None
    };
    let unchanged = previous.as_ref().is_some_and(|p| {
        p["prepared_digest"] == digest && p["error"] == error && p["questions"] == questions
    });
    super::super::source_store::save(
        &root,
        &name,
        &json!({"decision":request.id,"worker":request.worker,"error":error,
        "prepared_digest":digest,"original":job.state,"questions":job.questions,"selection":manifest,"prepared_state":prepared,
        "state":if unchanged{"unconfirmed_context"}else{"thinker_revision_required"},"at_ms":kah_observer::session::now_ms()}),
    )?;
    Ok(unchanged)
}
pub fn park(
    store: &Store,
    state: &mut super::super::state::State,
    request: &Request,
    result: &Value,
) -> Result<()> {
    let error = result["jev_state"]["unconfirmed_context"]
        .as_str()
        .unwrap_or("PREPARATION_CONTEXT_UNCONFIRMED");
    let observation = format!("context-observation-{}", request.id);
    let value = json!({"id":observation,"request":request.id,"worker":request.worker,"head":request.head,"reason":error,
        "kind":"unconfirmed_context","passed":null,"decision_not_confirmed":true,"no_automatic_reasks":true,
        "source":"context-rejections","next_step":"Observer-owned evidence/context revision on a new event; no user status confirmation."});
    super::super::source_store::save(&store.directory, &format!("{observation}.json"), &value)?;
    kah_core::run_store::ensure_private_directory(&store.directory.join("parked"))?;
    super::super::source_store::save(
        &store.directory.join("parked"),
        &format!("{}.json", request.id),
        &json!({"request":request.id,"observation":observation,"unconfirmed_observation":true,"reason":error}),
    )?;
    let mut result = result.clone();
    if result["assessment"]["violations"]
        .as_array()
        .is_some_and(|v| !v.is_empty())
    {
        result["assessment"]["passed"] = json!(false);
    } else {
        result["assessment"]["passed"] = Value::Null;
    }
    result["assessment"]["unconfirmed_context_observation"] = json!(observation);
    super::super::consume::archive(&store.directory, &request.id, &result)?;
    state.latest = Some(result);
    state.pending = None;
    store.save(state)?;
    super::super::source_store::finish_source_view(&state.observer.workspace, &request.id)?;
    super::super::reminder_queue::save(
        &store.directory,
        &json!({"request":request.id,"purpose":"uncertainty","turn":request.turn,"system_notice":true,
        "text":format!("kah observer retains {observation}: a repeated identical whole context is rejected by Jev's real token budget. Affected decision: {error}. This decision is unconfirmed, not passed. Original context/selection remains archived; unrelated reviews continue.")}),
    )?;
    Ok(())
}
