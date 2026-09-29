//! Provider-neutral intent and continuity rules.
pub mod ledger;
pub use ledger::{ClosureError, Ledger, LedgerError, RequirementStatus, Snapshot};
pub mod intent_history;
pub mod projection;
