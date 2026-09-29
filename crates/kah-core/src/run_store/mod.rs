//! Local, durable storage for one v2 intent run.
//!
//! This module is intentionally below the CLI boundary.  It owns the XDG
//! state layout and its crash-safe projection/history commit protocol, while
//! [`crate::intent_history::History`] and [`crate::projection::ActiveProjection`]
//! remain the authorities for event and projection validity.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::intent_history::{EventRecord, History, HistoryError};
use crate::projection::{ActiveProjection, ProjectionError, project};

const APPLICATION_DIRECTORY: &str = "kah";
const REPOSITORIES_DIRECTORY: &str = "repositories";
const RUNS_DIRECTORY: &str = "runs";
const HISTORY_DIRECTORY: &str = "history";
const PROJECTION_FILE: &str = "projection.json";
const SOURCE_FILE: &str = "source.json";
const CANDIDATE_FILE: &str = "candidate.json";
const COMPLETION_GATES_FILE: &str = "completion-gates.json";
const PENDING_FILE: &str = "pending.jsonl";
const LOCK_FILE: &str = ".writer.lock";
const TEMP_PROJECTION_PREFIX: &str = ".projection.json.tmp-";
const TEMP_SOURCE_PREFIX: &str = ".source.json.tmp-";
const TEMP_CANDIDATE_PREFIX: &str = ".candidate.json.tmp-";
const TEMP_COMPLETION_GATES_PREFIX: &str = ".completion-gates.json.tmp-";
const MAX_SOURCE_CURSOR_IDENTIFIER_BYTES: usize = 16 * 1024;

/// Exact schema accepted for the private source cursor state.
pub const SOURCE_CURSOR_SCHEMA_VERSION: u32 = 1;
pub const CANDIDATE_BINDING_SCHEMA_VERSION: u32 = 1;
pub const COMPLETION_GATE_STATE_SCHEMA_VERSION: u32 = 1;

static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

mod candidate;
mod checkpoint;
mod commit;
mod completion;
mod cursor;
mod error;
mod files;
mod filesystem;
mod identity;
mod lifecycle;
mod paths;
mod recovery;
mod validation;
mod writes;
pub use checkpoint::*;
pub use cursor::*;
pub use error::*;
use files::*;
pub use filesystem::ensure_private_directory;
use filesystem::*;
pub use identity::*;
pub use validation::repository_id;
use validation::*;

/// A locked local state directory for one repository/run pair.
#[derive(Debug)]
pub struct RunStore {
    path: PathBuf,
    _lock: File,
}

impl Drop for RunStore {
    fn drop(&mut self) {
        let _ = self._lock.unlock();
    }
}
