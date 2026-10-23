//! Requirement-specific external command verification.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod digest;
mod error;
use digest::*;
pub use error::VerificationError;

static ARTIFACT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub struct VerificationRequest {
    candidate: PathBuf,
    requirement_id: String,
    command: Vec<OsString>,
    artifact_root: PathBuf,
}

impl VerificationRequest {
    pub fn new(
        candidate: impl AsRef<Path>,
        requirement_id: impl Into<String>,
        command: impl IntoIterator<Item = OsString>,
        artifact_root: impl AsRef<Path>,
    ) -> Result<Self, VerificationError> {
        let candidate = candidate
            .as_ref()
            .canonicalize()
            .map_err(|error| VerificationError::io("canonicalize candidate", error))?;
        if !candidate.is_dir() {
            return Err(VerificationError::invalid(
                "VERIFICATION_CANDIDATE",
                "candidate must be a directory",
            ));
        }
        let requirement_id = requirement_id.into();
        if requirement_id.trim().is_empty()
            || requirement_id.len() > 512
            || requirement_id.chars().any(char::is_control)
        {
            return Err(VerificationError::invalid(
                "VERIFICATION_REQUIREMENT",
                "invalid requirement ID",
            ));
        }
        let command = command.into_iter().collect::<Vec<_>>();
        if command.is_empty() || command.iter().any(|argument| argument.is_empty()) {
            return Err(VerificationError::invalid(
                "VERIFICATION_COMMAND",
                "verification command must not be empty",
            ));
        }
        Ok(Self {
            candidate,
            requirement_id,
            command,
            artifact_root: artifact_root.as_ref().to_path_buf(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    Passed,
    Failed,
    ProtectedInputChanged,
    CandidateChanged,
}

impl VerificationStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::ProtectedInputChanged => "protected_input_changed",
            Self::CandidateChanged => "candidate_changed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct VerificationResult {
    pub schema_version: u32,
    pub requirement_id: String,
    pub status: VerificationStatus,
    pub exit_code: Option<i32>,
    pub command_digest: String,
    pub verifier_digest: String,
    pub candidate_before_digest: String,
    pub candidate_after_digest: String,
    pub stdout_digest: String,
    pub stderr_digest: String,
    pub artifact_id: String,
    pub started_at_unix_ms: u128,
    pub elapsed_ms: u128,
}

impl VerificationResult {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.status == VerificationStatus::Passed
    }
}

pub fn verify(request: &VerificationRequest) -> Result<VerificationResult, VerificationError> {
    let verifier_before = verifier_digest(&request.candidate, &request.command)?;
    let candidate_before = candidate_digest(&request.candidate)?;
    ensure_private_directory(&request.artifact_root)?;
    let started_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            VerificationError::invalid("VERIFICATION_CLOCK", "system clock is before Unix epoch")
        })?
        .as_millis();
    let started = Instant::now();
    let output = Command::new(&request.command[0])
        .args(&request.command[1..])
        .current_dir(&request.candidate)
        .output()
        .map_err(|error| VerificationError::io("execute verifier", error))?;
    let elapsed_ms = started.elapsed().as_millis();
    let stdout_digest = digest_bytes(&output.stdout);
    let stderr_digest = digest_bytes(&output.stderr);
    let command_digest = command_digest(&request.command);
    let artifact_id = format!(
        "verification-{}-{}-{}",
        std::process::id(),
        started_at_unix_ms,
        ARTIFACT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    write_artifacts(
        &request.artifact_root.join(&artifact_id),
        &request.command,
        &output.stdout,
        &output.stderr,
    )?;
    // The output is kept before any fallible digest of the candidate afterwards.
    let verifier_after = verifier_digest(&request.candidate, &request.command)?;
    let candidate_after = candidate_digest(&request.candidate)?;
    let status = if verifier_before != verifier_after {
        VerificationStatus::ProtectedInputChanged
    } else if candidate_before != candidate_after {
        VerificationStatus::CandidateChanged
    } else if output.status.success() {
        VerificationStatus::Passed
    } else {
        VerificationStatus::Failed
    };
    Ok(VerificationResult {
        schema_version: 1,
        requirement_id: request.requirement_id.clone(),
        status,
        exit_code: output.status.code(),
        command_digest,
        verifier_digest: verifier_before,
        candidate_before_digest: candidate_before,
        candidate_after_digest: candidate_after,
        stdout_digest,
        stderr_digest,
        artifact_id,
        started_at_unix_ms,
        elapsed_ms,
    })
}

fn ensure_private_directory(path: &Path) -> Result<(), VerificationError> {
    crate::run_store::ensure_private_directory(path)
        .map_err(|error| VerificationError::invalid("VERIFICATION_STATE", error.to_string()))
}

fn write_artifacts(
    directory: &Path,
    command: &[OsString],
    stdout: &[u8],
    stderr: &[u8],
) -> Result<(), VerificationError> {
    fs::create_dir(directory)
        .map_err(|error| VerificationError::io("create verification artifact", error))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
            .map_err(|error| VerificationError::io("protect verification artifact", error))?;
    }
    let command = command
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let metadata = serde_json::to_vec(&serde_json::json!({"schema_version":1,"command":command}))
        .map_err(|_| {
        VerificationError::invalid("VERIFICATION_JSON", "could not encode verifier metadata")
    })?;
    write_private_file(&directory.join("command.json"), &metadata)?;
    write_private_file(&directory.join("stdout.bin"), stdout)?;
    write_private_file(&directory.join("stderr.bin"), stderr)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), VerificationError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| VerificationError::io("create verification artifact file", error))?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all())
        .map_err(|error| VerificationError::io("write verification artifact file", error))?;
    File::open(path.parent().expect("artifact file has parent"))
        .and_then(|directory| directory.sync_all())
        .map_err(|error| VerificationError::io("sync verification artifact directory", error))
}
