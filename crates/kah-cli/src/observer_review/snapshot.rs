//! Immutable host snapshots share durable dispatch with ordinary review events.
use super::{
    queue, runtime,
    state::{Request, State, Store},
    workspace,
};
use crate::Result;
use serde_json::{Value, json};
use std::path::PathBuf;
pub fn wake(request: &Request) -> Result<Value> {
    runtime::require_active(&request.worker)?;
    queue::enqueue(
        &request.worker,
        request.purpose.clone(),
        &request.turn,
        json!({"captured_snapshot":request}),
    )?;
    queue::drain(&request.worker)
}
pub(super) fn submit(worker: &str, value: &Value) -> Result<Value> {
    let request: Request = serde_json::from_value(value.clone())?;
    if request.worker != worker {
        return Err("REVIEW_SNAPSHOT_BINDING stage=review".into());
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=review")?);
    let store = Store::open(worker)?;
    let state = match store.load()? {
        Some(state) => state,
        None => State::new(crate::relay_store::endpoint(worker)?, &home)?,
    };
    if state.pending.is_some() {
        return Ok(json!({"status":"queued","pending":state.pending}));
    }
    workspace::prepare(&home, &state)?;
    runtime::submit_snapshot(store, state, &request, &home)
}
