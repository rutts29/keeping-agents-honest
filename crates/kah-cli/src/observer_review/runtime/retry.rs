use super::*;
use crate::observer_review as review;
pub fn retry_pending(worker: &str) -> Result<()> {
    require_active(worker)?;
    let store = Store::open_wait(worker)?;
    let Some(state) = store.load()? else {
        return Ok(());
    };
    let Some(pending) = &state.pending else {
        return Ok(());
    };
    let gap_path = store.directory.join("gap.json");
    if working(&store, &state, &pending.id, &gap_path)? {
        return Ok(());
    }
    let gap: Option<Value> = review::state::read(&gap_path)?;
    if !gap.as_ref().is_some_and(|g| {
        g["error"]
            .as_str()
            .is_some_and(review::redelivery::retryable)
    }) {
        return Ok(());
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
    prompt.push_str(&format!("\nController validation diagnostic (not user steering): {}. Correct that report error before resubmitting the same request; preserve its original deferral anchors from intent.json.",gap.as_ref().unwrap()["error"]));
    let number = review::redelivery::begin(
        &store,
        &request.id,
        gap.as_ref().unwrap()["error"]
            .as_str()
            .unwrap_or("missing evidence report"),
    )?;
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
    if review::state::read::<Value>(&gap_path)? == gap {
        std::fs::remove_file(gap_path)?;
    }
    Ok(())
}
fn working(store: &Store, state: &State, request: &str, gap: &Path) -> Result<bool> {
    if !review::delivery::in_progress(state).unwrap_or(false) {
        return Ok(false);
    }
    if gap.exists() {
        std::fs::remove_file(gap)?;
    }
    review::redelivery::completed(store, request)?;
    Ok(true)
}
