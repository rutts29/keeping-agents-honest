//! Intent observation and deterministic proposal validation.
pub mod engine;
pub mod hook_event_store;
pub mod inspect_state;
pub mod session;
pub mod tool_watch;

pub mod judge;

pub mod compaction;
pub mod evaluation;
pub mod intake;
pub mod review;
pub mod verbatim;
