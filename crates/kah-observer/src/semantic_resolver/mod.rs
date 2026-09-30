//! Source-linked semantic proposal generation through the installed Codex CLI.
//!
//! Model output is never trusted as history. The resolver constrains the final
//! response syntactically, validates every source and transition locally, and
//! only then builds the public v2 history and bounded active projection.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use kah_core::intent_history::{
    Conflict, ConflictAlternative, Event, EvidenceId, GoalRevision, History, IntentId, IntentItem,
    IntentKind, SourceRef,
};
use kah_core::projection::{ActiveProjection, ProjectionConfig};
use kah_core::session_bundle::{BundleRecord, NativeGoal, NeutralSessionBundle};

const PROPOSAL_SCHEMA_VERSION: u32 = 1;
const PROMPT_SCHEMA_VERSION: u32 = 4;
const INCREMENTAL_PROMPT_SCHEMA_VERSION: u32 = 3;
const MAX_INCREMENTAL_DELTA_PROMPT_BYTES: usize = 64 * 1024;
static ARTIFACT_COUNTER: AtomicU64 = AtomicU64::new(0);

mod config;
pub use config::*;
mod types;
pub use types::*;
mod chunking;
pub use chunking::*;
mod initial;
pub use initial::*;
mod incremental;
pub use incremental::*;
mod metadata;
use metadata::*;
mod backend;
use backend::*;
mod prompts;
use prompts::*;
mod incremental_validation;
use incremental_validation::*;
mod incremental_rules;
use incremental_rules::*;
mod provenance;
use provenance::*;
mod artifacts;
use artifacts::*;
mod schema;
use schema::*;
mod initial_validation;
use initial_validation::*;
mod validation;
use validation::*;
mod render;
pub use render::*;

#[cfg(test)]
mod tests;

mod incremental_operations;
mod initial_operations;
mod operations;
use operations::*;

mod attempt;
use attempt::*;

mod execution_error;
use execution_error::*;
