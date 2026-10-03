//! Intent observation and deterministic proposal validation.
pub mod engine;
pub mod hook_event_store;
pub mod inspect_state;
pub mod session;
pub mod tool_watch;

pub mod judge;

pub mod checkpoint;
pub mod compaction;
pub mod evaluation;
pub mod inherited;
pub mod intake;
pub mod relayed_user;
pub mod review;
pub mod review_evidence;
pub mod verbatim;
pub mod verification;
pub mod verified_judgment;
