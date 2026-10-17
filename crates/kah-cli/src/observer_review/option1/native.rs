//! Option1 jobs use the existing full-TUI lifecycle and validated host hooks.
use super::super::{
    runtime,
    state::{Purpose, Request, State, Store},
};
use crate::Result;
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use std::path::Path;
pub fn submit(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    head: &str,
    job: Value,
    trigger: Value,
) -> Result<Value> {
    let job =
        super::super::execution_fact::attach(job, super::super::execution_fact::fact(home, &state));
    let mut trigger = trigger;
    trigger["option1"] = job.clone();
    let mut request = Request {
        id: format!(
            "review-{}-{}",
            kah_observer::session::now_ms(),
            std::process::id()
        ),
        worker: state.worker.session.clone(),
        purpose: if job["stage"] == "current_rules" { Purpose::Steering } else { Purpose::Checkpoint },
        turn: turn.into(),
        head: Some(head.into()),
        intent: job.get("intent").cloned().unwrap_or_else(||json!({"revision":job["input"]["carried"]["intent_revision"],"goal":job["input"]["intent"]["goal"]})),
        trigger,
        documents: vec![Document::new(
            "option1",
            "literal_first",
            "kah:option1",
            &serde_json::to_string(&job)?,
        )],
        created_ms: kah_observer::session::now_ms(),
    };
    if job["stage"] == "checkpoint" {
        request
            .documents
            .push(super::super::capture::history(home, &state)?);
    }
    runtime::submit_snapshot(store, state, &request, home)
}
