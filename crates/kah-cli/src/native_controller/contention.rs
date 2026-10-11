//! Controller polls retain an active native run across shared writer contention.
use crate::Result;
pub(super) fn poll<T>(operation: impl FnOnce() -> Result<T>) -> Result<Option<T>> {
    match operation() {
        Ok(value) => Ok(Some(value)),
        Err(error) if busy(&error.to_string()) => Ok(None),
        Err(error) => Err(error),
    }
}
fn busy(error: &str) -> bool {
    let error = error
        .strip_prefix("RUN_OBSERVER_GAP stage=completion: ")
        .unwrap_or(error);
    [
        "OBSERVER_BUSY stage=session_store",
        "STORE_BUSY stage=run_store",
        "RUN_BUSY stage=run_store",
    ]
    .iter()
    .any(|code| {
        error
            .strip_prefix(code)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(':'))
    })
}
#[cfg(test)]
#[path = "../../tests/observer_unit/native_startup_contention.rs"]
mod tests;
