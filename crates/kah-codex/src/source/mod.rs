use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use kah_core::intent_history::{SourceProvider, SourceRole};
use kah_core::session_bundle::{BundleRecord, NativeGoal, NeutralSessionBundle};

const MAX_PROTOCOL_LINE_BYTES: usize = 128 * 1024 * 1024;
const MAX_THREAD_LIST_PAGES: usize = 100;
const THREAD_SOURCE_KINDS: [&str; 10] = [
    "cli",
    "vscode",
    "exec",
    "appServer",
    "subAgent",
    "subAgentReview",
    "subAgentCompact",
    "subAgentThreadSpawn",
    "subAgentOther",
    "unknown",
];

mod models;
pub use models::*;
mod paths;
use paths::*;
mod error;
pub use error::*;
mod inspect;
pub use inspect::*;
mod observation;
pub use observation::*;
mod server;
use server::*;
mod listing;
use listing::*;
mod session;
use session::*;
mod turns;
use turns::*;
mod items;
use items::*;
mod goal;
use goal::*;
mod fields;
use fields::*;

#[cfg(test)]
mod tests;
