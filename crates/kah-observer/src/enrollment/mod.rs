//! Private activation records for exact provider sessions.

use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

const SCHEMA_VERSION: u32 = 1;
const MAX_SESSION_ID_BYTES: usize = 512;
static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enrollment {
    schema_version: u32,
    provider: String,
    session_id: String,
    repository: PathBuf,
    run_id: String,
    allow_ancestor_cwd: bool,
    #[serde(default = "default_keeper_preset")]
    keeper_preset: String,
    #[serde(default)]
    keeper_reasoning: Option<String>,
}

impl Enrollment {
    pub fn new(
        session_id: &str,
        repository: &Path,
        run_id: &str,
        allow_ancestor_cwd: bool,
        keeper_preset: &str,
        keeper_reasoning: Option<&str>,
    ) -> Result<Self, EnrollmentError> {
        validate_session_id(session_id)?;
        kah_core::run_store::RunId::parse(run_id)?;
        crate::semantic_resolver::CompactorConfig::new(keeper_preset, keeper_reasoning)
            .map_err(|_| EnrollmentError::Invalid("invalid keeper model policy"))?;
        let repository = repository
            .canonicalize()
            .map_err(|error| io_error("canonicalize attached repository", repository, error))?;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            provider: "codex".to_owned(),
            session_id: session_id.to_owned(),
            repository,
            run_id: run_id.to_owned(),
            allow_ancestor_cwd,
            keeper_preset: keeper_preset.to_owned(),
            keeper_reasoning: keeper_reasoning.map(str::to_owned),
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn repository(&self) -> &Path {
        &self.repository
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn allow_ancestor_cwd(&self) -> bool {
        self.allow_ancestor_cwd
    }

    pub fn keeper_config(
        &self,
    ) -> Result<crate::semantic_resolver::CompactorConfig, EnrollmentError> {
        crate::semantic_resolver::CompactorConfig::new(
            &self.keeper_preset,
            self.keeper_reasoning.as_deref(),
        )
        .map_err(|_| EnrollmentError::Invalid("invalid keeper model policy"))
    }

    pub fn validates_hook_cwd(&self, cwd: &Path) -> Result<bool, EnrollmentError> {
        let cwd = cwd
            .canonicalize()
            .map_err(|error| io_error("canonicalize hook working directory", cwd, error))?;
        Ok(
            cwd == self.repository
                || (self.allow_ancestor_cwd && self.repository.starts_with(&cwd)),
        )
    }
}

impl fmt::Debug for Enrollment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Enrollment")
            .field("provider", &self.provider)
            .field("session", &redacted_session(&self.session_id))
            .field("run_id", &self.run_id)
            .field("allow_ancestor_cwd", &self.allow_ancestor_cwd)
            .field("keeper_preset", &self.keeper_preset)
            .field("keeper_reasoning", &self.keeper_reasoning)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub enum EnrollmentError {
    Invalid(&'static str),
    Conflict,
    Io {
        action: &'static str,
        path: PathBuf,
        message: String,
    },
    Serialization(String),
    RunStore(kah_core::run_store::RunStoreError),
}

impl fmt::Display for EnrollmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage=enrollment: ", self.code())?;
        match self {
            Self::Invalid(reason) => formatter.write_str(reason),
            Self::Conflict => formatter
                .write_str("this exact Codex session already has a different Driftctl enrollment"),
            Self::Io {
                action,
                path,
                message,
            } => write!(
                formatter,
                "could not {action} {}: {message}",
                path.display()
            ),
            Self::Serialization(message) => {
                write!(formatter, "invalid enrollment state: {message}")
            }
            Self::RunStore(error) => write!(formatter, "invalid enrollment run: {error}"),
        }
    }
}

impl std::error::Error for EnrollmentError {}

impl From<kah_core::run_store::RunStoreError> for EnrollmentError {
    fn from(error: kah_core::run_store::RunStoreError) -> Self {
        Self::RunStore(error)
    }
}

pub fn state_root() -> Result<PathBuf, EnrollmentError> {
    let base = match env::var_os("XDG_STATE_HOME") {
        Some(path) if Path::new(&path).is_absolute() => PathBuf::from(path),
        _ => {
            let home = env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .ok_or(EnrollmentError::Invalid("HOME is not set"))?;
            PathBuf::from(home).join(".local/state")
        }
    };
    Ok(base.join("kah"))
}

pub fn optional_state_root() -> Option<PathBuf> {
    state_root().ok()
}

pub fn load(session_id: &str) -> Result<Option<Enrollment>, EnrollmentError> {
    validate_session_id(session_id)?;
    let Some(root) = optional_state_root() else {
        return Ok(None);
    };
    let path = enrollment_path(&root, session_id);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(io_error("inspect enrollment", &path, error)),
    };
    ensure_private_regular(&path, &metadata)?;
    let bytes = fs::read(&path).map_err(|error| io_error("read enrollment", &path, error))?;
    let enrollment: Enrollment = serde_json::from_slice(&bytes)
        .map_err(|error| EnrollmentError::Serialization(error.to_string()))?;
    enrollment.validate(session_id, &path)?;
    Ok(Some(enrollment))
}

/// Persist one exact enrollment. Returns `true` only for a newly created record.
pub fn attach(enrollment: &Enrollment) -> Result<bool, EnrollmentError> {
    if let Some(existing) = load(enrollment.session_id())? {
        return if existing == *enrollment {
            Ok(false)
        } else {
            Err(EnrollmentError::Conflict)
        };
    }
    let root = state_root()?;
    create_private_directory(&root)?;
    let directory = root.join("enrollments");
    create_private_directory(&directory)?;
    let path = enrollment_path(&root, enrollment.session_id());
    let temporary = directory.join(format!(
        ".enrollment.tmp-{}-{}",
        std::process::id(),
        TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let bytes = serde_json::to_vec(enrollment)
        .map_err(|error| EnrollmentError::Serialization(error.to_string()))?;
    write_private_new(&temporary, &bytes)?;
    match fs::rename(&temporary, &path) {
        Ok(()) => {
            sync_directory(&directory)?;
            Ok(true)
        }
        Err(error) => {
            let _ = fs::remove_file(&temporary);
            if error.kind() == io::ErrorKind::AlreadyExists {
                match load(enrollment.session_id())? {
                    Some(existing) if existing == *enrollment => Ok(false),
                    _ => Err(EnrollmentError::Conflict),
                }
            } else {
                Err(io_error("install enrollment", &path, error))
            }
        }
    }
}

pub fn detach(session_id: &str, expected_repository: &Path) -> Result<bool, EnrollmentError> {
    let Some(enrollment) = load(session_id)? else {
        return Ok(false);
    };
    let repository = expected_repository.canonicalize().map_err(|error| {
        io_error(
            "canonicalize repository before detach",
            expected_repository,
            error,
        )
    })?;
    if enrollment.repository != repository {
        return Err(EnrollmentError::Invalid(
            "the exact session is attached to a different repository",
        ));
    }
    let root = state_root()?;
    let path = enrollment_path(&root, session_id);
    fs::remove_file(&path).map_err(|error| io_error("remove enrollment", &path, error))?;
    sync_directory(
        path.parent()
            .ok_or(EnrollmentError::Invalid("invalid enrollment path"))?,
    )?;
    Ok(true)
}

pub fn redacted_session(session_id: &str) -> String {
    let digest = Sha256::digest(session_id.as_bytes());
    let opaque = digest[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("codex-session:sha256:{opaque}")
}

fn enrollment_path(root: &Path, session_id: &str) -> PathBuf {
    let digest = Sha256::digest(session_id.as_bytes());
    root.join("enrollments").join(format!("{digest:x}.json"))
}

fn validate_session_id(session_id: &str) -> Result<(), EnrollmentError> {
    if session_id.trim().is_empty()
        || session_id.len() > MAX_SESSION_ID_BYTES
        || session_id.chars().any(char::is_control)
    {
        Err(EnrollmentError::Invalid(
            "invalid explicit Codex session ID",
        ))
    } else {
        Ok(())
    }
}

impl Enrollment {
    fn validate(&self, expected_session: &str, path: &Path) -> Result<(), EnrollmentError> {
        if self.schema_version != SCHEMA_VERSION
            || self.provider != "codex"
            || self.session_id != expected_session
            || self.repository.as_os_str().is_empty()
            || !self.repository.is_absolute()
        {
            return Err(EnrollmentError::Io {
                action: "validate enrollment",
                path: path.to_owned(),
                message: "record does not match the exact requested session".to_owned(),
            });
        }
        validate_session_id(&self.session_id)?;
        kah_core::run_store::RunId::parse(&self.run_id)?;
        self.keeper_config()?;
        Ok(())
    }
}

fn default_keeper_preset() -> String {
    "luna".to_owned()
}

mod files;
use files::*;
#[cfg(test)]
mod tests;

impl EnrollmentError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(..) => "ENROLLMENT_INVALID",
            Self::Conflict => "ENROLLMENT_CONFLICT",
            Self::Io { .. } => "ENROLLMENT_IO",
            Self::Serialization(..) => "ENROLLMENT_SERIALIZATION",
            Self::RunStore(..) => "ENROLLMENT_RUN_STORE",
        }
    }
}
