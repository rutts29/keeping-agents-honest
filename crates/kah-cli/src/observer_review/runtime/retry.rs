use super::*;
use crate::observer_review as review;
pub fn retry_pending(worker: &str) -> Result<()> {
    require_active(worker)?;
    if review::service::blocked(worker)? {
        return Ok(());
    }
    let _waiting = review::liveness::waiting(worker)?;
    let store = Store::open_wait(worker)?;
    let Some(mut state) = store.load()? else {
        return Ok(());
    };
    if review::checkpoint_claim::release_waiting(&store, &mut state)? {
        drop(store);
        return review::queue::kick(worker);
    }
    let Some(pending) = &state.pending else {
        return Ok(());
    };
    let gap_path = store.directory.join("gap.json");
    refresh_rejection(&store, &state, &pending.id)?;
    if working(&store, &state, &pending.id, &gap_path)? {
        return Ok(());
    }
    let gap: Option<Value> = review::state::read(&gap_path)?;
    if !retry_due(&store.directory, &pending.id, gap.as_ref())? {
        return Ok(());
    }
    if review::redelivery::unchanged_correction(&store, &pending.id, gap.as_ref().unwrap())? {
        review::redelivery::park_correction(&store, &pending.id, gap.as_ref().unwrap())?;
        drop(store);
        review::redelivery::notify(&state)?;
        return review::queue::kick(worker);
    }
    let request = workspace::request(&store, &pending.id)?;
    let _processing = review::liveness::processing(worker, &pending.id)?;
    let decision = review::preparation::active_decision(worker, &pending.id)?;
    transport(&state)?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("REVIEW_HOME stage=retry")?);
    let mut prompt = review::redelivery::prompt(&home, &store, &state, &request, &decision)?;
    // Publication can overlap a late native acceptance. Recheck before any
    // failure counter/notice; a working accepted request needs no redelivery.
    if working(&store, &state, &request.id, &gap_path)? {
        return Ok(());
    }
    prompt.push_str(&diagnostic(gap.as_ref().unwrap()));
    let number = review::redelivery::begin_correction(&store, &request.id, gap.as_ref().unwrap())?;
    drop(store);
    let delivery = deliver(&home, &state, &prompt, true);
    let store = Store::open_wait(worker)?;
    review::redelivery::outcome(
        &store,
        &request.id,
        number,
        delivery.as_ref().err().map(ToString::to_string),
    )?;
    if delivery.is_ok() {
        review::redelivery::completed(&store, &request.id)?;
    }
    drop(store);
    review::redelivery::notify(&state)?;
    delivery?;
    clear_same_gap(&gap_path, &gap)
}
fn retry_due(root: &Path, request: &str, gap: Option<&Value>) -> Result<bool> {
    let Some(gap) = gap.filter(|g| review::redelivery::gap_retryable(g)) else {
        return Ok(false);
    };
    if gap["error"].as_str().is_some_and(|e| {
        e.starts_with("REVIEW_SOURCE_OUTSIDE_PACKET") || e.starts_with("REVIEW_EXPANSION_PENDING")
    }) {
        return review::redelivery::expansion_retry::due(root, request);
    }
    Ok(true)
}
fn clear_same_gap(path: &Path, gap: &Option<Value>) -> Result<()> {
    if review::state::read::<Value>(path)? == *gap {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
fn diagnostic(gap: &Value) -> String {
    if gap["error"].as_str().is_some_and(|e| {
        e.starts_with("REVIEW_SOURCE_OUTSIDE_PACKET") || e.starts_with("REVIEW_EXPANSION_PENDING")
    }) {
        "\nHost expansion completed: the complete cited file is now mapped in this SAME request. Re-read its manifest and resubmit your source-checked report; this was a host publication handoff, not a report mistake.".into()
    } else {
        format!(
            "\nController validation diagnostic (not user steering): {}. Correct that report error before resubmitting the same request; preserve its original deferral anchors from intent.json.",
            gap["error"]
        )
    }
}
fn refresh_rejection(store: &Store, state: &State, request: &str) -> Result<()> {
    let prior: Option<Value> =
        review::state::read(&store.directory.join("validation-current.json"))?;
    if review::liveness::rejection::current(
        &store.directory,
        &state.observer.workspace.join("outbox"),
        &state.worker.session,
        request,
    )?
    .is_none()
    {
        return Ok(());
    }
    let record: Value = review::state::read(&store.directory.join("validation-current.json"))?
        .ok_or("REVIEW_REJECTION_MISSING stage=host_handoff")?;
    if record["request"] == request {
        let existing: Option<Value> = review::state::read(&store.directory.join("gap.json"))?;
        if existing.as_ref().is_some_and(|v| {
            v["request"] == request
                && v["error"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("REVIEW_EXPANSION_PENDING"))
                && prior.as_ref().is_some_and(|p| {
                    p["request_digest"] == record["request_digest"]
                        && p["report_digest"] == record["report_digest"]
                })
        }) {
            return Ok(());
        }
        crate::unattended_report::save(&store.directory, "gap.json", &record)?;
    }
    Ok(())
}
fn working(store: &Store, state: &State, request: &str, gap: &Path) -> Result<bool> {
    if review::liveness::awaiting_host(state)? {
        clear_confirmed_delivery_gap(gap)?;
        review::hook::spawn("__review-consume", &state.worker.session)?;
        return Ok(true);
    }
    if !review::delivery::in_progress(state).unwrap_or(false) {
        return Ok(false);
    }
    clear_confirmed_delivery_gap(gap)?;
    review::redelivery::completed(store, request)?;
    Ok(true)
}

fn clear_confirmed_delivery_gap(path: &Path) -> Result<()> {
    let gap: Option<Value> = review::state::read(path)?;
    if gap
        .as_ref()
        .and_then(|g| g["error"].as_str())
        .is_some_and(|error| {
            review::delivery::wait_timeout(error)
                || [
                    "REVIEW_SUBMISSION_UNCONFIRMED",
                    "REVIEW_NOT_IDLE",
                    "REVIEW_OBSERVER_STALLED",
                ]
                .iter()
                .any(|prefix| error.starts_with(prefix))
        })
    {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/retry_gap.rs"]
mod tests;
