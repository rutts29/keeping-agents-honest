//! Opt-in, per-thread record of integrity-checked source reads.
use std::cell::RefCell;
thread_local! {
    static READS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}
pub(super) fn note(digest: &str) {
    READS.with(|reads| {
        if let Some(reads) = reads.borrow_mut().as_mut() {
            reads.push(digest.into());
        }
    });
}
/// Runs `operation` and returns each source digest it opened, in read order.
pub fn record<T>(operation: impl FnOnce() -> T) -> (T, Vec<String>) {
    let previous = READS.with(|reads| reads.replace(Some(Vec::new())));
    let value = operation();
    let reads = READS.with(|reads| reads.replace(previous));
    (value, reads.unwrap_or_default())
}
