//! Strict, provider-neutral intake for local session transcripts.
//!
//! This module intentionally only preserves normalized source records.  It
//! does not infer intent, build a projection, or grant authority beyond
//! identifying explicit user records.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::intent_history::{SourceProvider, SourceRef, SourceRole};

/// The only neutral-session-bundle schema this release accepts.
pub const SESSION_BUNDLE_SCHEMA_VERSION: u32 = 1;

const MAX_IDENTIFIER_BYTES: usize = 16 * 1024;
const MAX_CONTENT_BYTES: usize = 1024 * 1024;

mod goal;
mod records;
mod validation;
mod wire;
pub use goal::*;
pub use records::*;
use validation::*;
use wire::*;

/// A strict, versioned session intake document.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct NeutralSessionBundle {
    schema_version: u32,
    source: BundleSource,
    native_goal: NativeGoal,
    records: Vec<BundleRecord>,
}

impl NeutralSessionBundle {
    /// Builds a bundle from provider records, deriving source head and digest.
    pub fn from_records(
        provider: SourceProvider,
        session_ref: impl Into<String>,
        repository_digest: impl Into<String>,
        records: Vec<BundleRecord>,
    ) -> Result<Self, SessionBundleError> {
        Self::from_records_with_native_goal(
            provider,
            session_ref,
            repository_digest,
            NativeGoal::Unknown,
            records,
        )
    }

    /// Builds a bundle with an already-observed native goal state.
    pub fn from_records_with_native_goal(
        provider: SourceProvider,
        session_ref: impl Into<String>,
        repository_digest: impl Into<String>,
        native_goal: NativeGoal,
        records: Vec<BundleRecord>,
    ) -> Result<Self, SessionBundleError> {
        validate_records(&records)?;
        native_goal.validate()?;
        let session_ref = session_ref.into();
        let repository_digest = repository_digest.into();
        validate_identifier(&session_ref, "source session_ref")?;
        validate_sha256(&repository_digest, "source repository_digest")?;
        let source = BundleSource {
            provider,
            session_ref,
            repository_digest,
            head: records
                .last()
                .expect("validated non-empty records")
                .id
                .clone(),
            digest: source_digest(&records),
        };
        Ok(Self {
            schema_version: SESSION_BUNDLE_SCHEMA_VERSION,
            source,
            native_goal,
            records,
        })
    }

    /// Parses an exact schema-v1 JSON document and validates every field.
    pub fn from_json(json: &str) -> Result<Self, SessionBundleError> {
        let wire: WireBundle = serde_json::from_str(json)
            .map_err(|error| SessionBundleError::Json(error.to_string()))?;
        Self::try_from(wire)
    }

    /// Emits the strict schema-v1 document for local handoff.
    pub fn to_json(&self) -> Result<String, SessionBundleError> {
        serde_json::to_string(self).map_err(|error| SessionBundleError::Json(error.to_string()))
    }

    #[must_use]
    pub fn source(&self) -> &BundleSource {
        &self.source
    }

    #[must_use]
    pub fn native_goal(&self) -> &NativeGoal {
        &self.native_goal
    }

    #[must_use]
    pub fn records(&self) -> &[BundleRecord] {
        &self.records
    }

    /// Only explicit user text may be used as semantic authority by a later
    /// resolver. Other roles remain available as source evidence/context.
    #[must_use]
    pub fn authoritative_records(&self) -> Vec<&BundleRecord> {
        self.records
            .iter()
            .filter(|record| record.is_authoritative())
            .collect()
    }

    /// Converts preserved records to the typed provenance references used by
    /// later history layers, without making any semantic decision.
    #[must_use]
    pub fn source_refs(&self) -> Vec<SourceRef> {
        self.records
            .iter()
            .map(|record| {
                SourceRef::new(
                    self.source.provider.clone(),
                    self.source.session_ref.clone(),
                    record.id.clone(),
                    record.role.clone(),
                    record.content_digest.clone(),
                )
            })
            .collect()
    }

    /// Enforces the additional precondition a projection resolver will need.
    pub fn validate_for_projection(&self) -> Result<(), SessionBundleError> {
        if self.authoritative_records().is_empty() {
            return Err(SessionBundleError::NoAuthoritativeUserRecord);
        }
        Ok(())
    }

    #[must_use]
    pub fn sanitized_summary(&self) -> SanitizedBundleSummary {
        SanitizedBundleSummary {
            schema_version: self.schema_version,
            provider: self.source.provider.clone(),
            repository_digest: self.source.repository_digest.clone(),
            source_head: self.source.head.clone(),
            source_digest: self.source.digest.clone(),
            native_goal_state: self.native_goal.state(),
            record_count: self.records.len(),
            authoritative_user_record_count: self.authoritative_records().len(),
        }
    }
}

impl fmt::Debug for NeutralSessionBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NeutralSessionBundle")
            .field("schema_version", &self.schema_version)
            .field("source", &self.source)
            .field("native_goal", &self.native_goal)
            .field("record_count", &self.records.len())
            .field(
                "authoritative_user_record_count",
                &self.authoritative_records().len(),
            )
            .finish()
    }
}

/// Rendering-safe metadata for logs and public inspection output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SanitizedBundleSummary {
    schema_version: u32,
    provider: SourceProvider,
    repository_digest: String,
    source_head: String,
    source_digest: String,
    native_goal_state: &'static str,
    record_count: usize,
    authoritative_user_record_count: usize,
}

impl fmt::Display for SanitizedBundleSummary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "schema_version: {}\nprovider: {:?}\nrepository_digest: {}\nsource_head: {}\nsource_digest: {}\nnative_goal_state: {}\nrecords: {}\nauthoritative_user_records: {}",
            self.schema_version,
            self.provider,
            self.repository_digest,
            self.source_head,
            self.source_digest,
            self.native_goal_state,
            self.record_count,
            self.authoritative_user_record_count,
        )
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum SessionBundleError {
    Json(String),
    UnsupportedSchema(u32),
    InvalidField(&'static str),
    InvalidDigest(&'static str),
    DuplicateRecordId(String),
    EmptyRecords,
    NoAuthoritativeUserRecord,
    SourceHeadMismatch,
    SourceDigestMismatch,
}

impl fmt::Display for SessionBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage=session_intake: ", self.code())?;
        match self {
            Self::Json(error) => write!(formatter, "invalid neutral session bundle JSON: {error}"),
            Self::UnsupportedSchema(version) => {
                write!(
                    formatter,
                    "unsupported neutral session bundle schema version: {version}"
                )
            }
            Self::InvalidField(field) => {
                write!(formatter, "invalid neutral session bundle field: {field}")
            }
            Self::InvalidDigest(field) => {
                write!(formatter, "invalid SHA-256 digest in field: {field}")
            }
            Self::DuplicateRecordId(id) => write!(
                formatter,
                "duplicate neutral session bundle record ID: {id}"
            ),
            Self::EmptyRecords => formatter.write_str("neutral session bundle has no records"),
            Self::NoAuthoritativeUserRecord => {
                formatter.write_str("neutral session bundle has no explicit user text")
            }
            Self::SourceHeadMismatch => {
                formatter.write_str("neutral session bundle source head does not match records")
            }
            Self::SourceDigestMismatch => {
                formatter.write_str("neutral session bundle source digest does not match records")
            }
        }
    }
}

impl std::error::Error for SessionBundleError {}

impl SessionBundleError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Json(..) => "BUNDLE_JSON",
            Self::UnsupportedSchema(..) => "BUNDLE_UNSUPPORTED_SCHEMA",
            Self::InvalidField(..) => "BUNDLE_INVALID_FIELD",
            Self::InvalidDigest(..) => "BUNDLE_INVALID_DIGEST",
            Self::DuplicateRecordId(..) => "BUNDLE_DUPLICATE_RECORD_ID",
            Self::EmptyRecords => "BUNDLE_EMPTY_RECORDS",
            Self::NoAuthoritativeUserRecord => "BUNDLE_NO_AUTHORITATIVE_USER_RECORD",
            Self::SourceHeadMismatch => "BUNDLE_SOURCE_HEAD_MISMATCH",
            Self::SourceDigestMismatch => "BUNDLE_SOURCE_DIGEST_MISMATCH",
        }
    }
}
