use super::*;

/// One accepted source position, retaining identity and digest but never the
/// provider transcript content itself.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecordDigest {
    pub(super) id: String,
    pub(super) content_digest: String,
    #[serde(default = "default_source_record_role")]
    pub(super) role: crate::intent_history::SourceRole,
}

impl SourceRecordDigest {
    #[must_use]
    pub fn new(id: impl Into<String>, content_digest: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content_digest: content_digest.into(),
            role: crate::intent_history::SourceRole::User,
        }
    }

    #[must_use]
    pub fn with_role(
        id: impl Into<String>,
        content_digest: impl Into<String>,
        role: crate::intent_history::SourceRole,
    ) -> Self {
        Self {
            id: id.into(),
            content_digest: content_digest.into(),
            role,
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }

    #[must_use]
    pub fn role(&self) -> &crate::intent_history::SourceRole {
        &self.role
    }
}

fn default_source_record_role() -> crate::intent_history::SourceRole {
    crate::intent_history::SourceRole::User
}

/// Private, digest-only cursor for the source records accepted by a run.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceCursor {
    pub(super) schema_version: u32,
    pub(super) provider: crate::intent_history::SourceProvider,
    pub(super) session_locator: String,
    pub(super) repository_digest: String,
    pub(super) accepted_records: Vec<SourceRecordDigest>,
    pub(super) head: String,
    pub(super) digest: String,
}

impl fmt::Debug for SourceCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceCursor")
            .field("schema_version", &self.schema_version)
            .field("provider", &self.provider)
            .field("repository_digest", &self.repository_digest)
            .field("accepted_records", &self.accepted_records)
            .field("head", &self.head)
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

impl SourceCursor {
    pub fn new(
        provider: crate::intent_history::SourceProvider,
        session_locator: impl Into<String>,
        repository_digest: impl Into<String>,
        accepted_records: Vec<SourceRecordDigest>,
    ) -> Result<Self, SourceCursorError> {
        let cursor = Self {
            schema_version: SOURCE_CURSOR_SCHEMA_VERSION,
            provider,
            session_locator: session_locator.into(),
            repository_digest: repository_digest.into(),
            head: accepted_records
                .last()
                .map(|record| record.id.clone())
                .unwrap_or_default(),
            digest: source_cursor_digest(&accepted_records),
            accepted_records,
        };
        cursor.validate()?;
        Ok(cursor)
    }

    #[must_use]
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    #[must_use]
    pub fn provider(&self) -> &crate::intent_history::SourceProvider {
        &self.provider
    }

    /// The provider locator is private state and intentionally omitted from
    /// [`fmt::Debug`] output.
    #[must_use]
    pub fn session_locator_private(&self) -> &str {
        &self.session_locator
    }

    #[must_use]
    pub fn repository_digest(&self) -> &str {
        &self.repository_digest
    }

    #[must_use]
    pub fn accepted_records(&self) -> &[SourceRecordDigest] {
        &self.accepted_records
    }

    #[must_use]
    pub fn accepted_record_count(&self) -> usize {
        self.accepted_records.len()
    }

    #[must_use]
    pub fn head(&self) -> &str {
        &self.head
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    pub fn compare(&self, observed: &Self) -> Result<SourceCursorComparison, SourceCursorError> {
        self.validate()?;
        observed.validate()?;
        if self.provider != observed.provider
            || self.session_locator != observed.session_locator
            || self.repository_digest != observed.repository_digest
        {
            return Ok(SourceCursorComparison::SessionMismatch);
        }

        for (index, accepted) in self.accepted_records.iter().enumerate() {
            let Some(current) = observed.accepted_records.get(index) else {
                return Ok(SourceCursorComparison::Stale {
                    accepted_head: self.head.clone(),
                    observed_head: observed.head.clone(),
                });
            };
            if accepted != current {
                return Ok(SourceCursorComparison::Rewrite {
                    accepted_record_id: accepted.id.clone(),
                    observed_record_id: current.id.clone(),
                });
            }
        }

        let new_records = observed.accepted_records[self.accepted_records.len()..].to_vec();
        if new_records.is_empty() {
            Ok(SourceCursorComparison::Current)
        } else {
            Ok(SourceCursorComparison::NewRecords(new_records))
        }
    }

    pub fn validate(&self) -> Result<(), SourceCursorError> {
        if self.schema_version != SOURCE_CURSOR_SCHEMA_VERSION {
            return Err(SourceCursorError::UnsupportedSchemaVersion {
                expected: SOURCE_CURSOR_SCHEMA_VERSION,
                found: self.schema_version,
            });
        }
        validate_source_identifier(&self.session_locator, "source session locator")?;
        validate_source_digest(&self.repository_digest, "source repository digest")?;
        if self.accepted_records.is_empty() {
            return Err(SourceCursorError::EmptyRecords);
        }
        let mut ids = BTreeSet::new();
        for record in &self.accepted_records {
            validate_source_identifier(&record.id, "source record id")?;
            validate_source_digest(&record.content_digest, "source record content digest")?;
            if !ids.insert(&record.id) {
                return Err(SourceCursorError::DuplicateRecordId(record.id.clone()));
            }
        }
        if self.head
            != self
                .accepted_records
                .last()
                .expect("checked nonempty records")
                .id
        {
            return Err(SourceCursorError::SourceHeadMismatch);
        }
        if self.digest != source_cursor_digest(&self.accepted_records) {
            return Err(SourceCursorError::SourceDigestMismatch);
        }
        Ok(())
    }
}

/// The exact-prefix relationship between accepted and currently observed
/// source records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceCursorComparison {
    Current,
    NewRecords(Vec<SourceRecordDigest>),
    Stale {
        accepted_head: String,
        observed_head: String,
    },
    Rewrite {
        accepted_record_id: String,
        observed_record_id: String,
    },
    SessionMismatch,
}

/// Invalid private source cursor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceCursorError {
    UnsupportedSchemaVersion { expected: u32, found: u32 },
    InvalidField(&'static str),
    InvalidDigest(&'static str),
    EmptyRecords,
    DuplicateRecordId(String),
    SourceHeadMismatch,
    SourceDigestMismatch,
}

impl fmt::Display for SourceCursorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} stage=source_cursor: ", self.code())?;
        match self {
            Self::UnsupportedSchemaVersion { expected, found } => write!(
                formatter,
                "unsupported source cursor schema version: expected {expected}, found {found}"
            ),
            Self::InvalidField(field) => write!(formatter, "invalid source cursor field: {field}"),
            Self::InvalidDigest(field) => {
                write!(formatter, "invalid source cursor digest: {field}")
            }
            Self::EmptyRecords => formatter.write_str("source cursor has no accepted records"),
            Self::DuplicateRecordId(id) => write!(formatter, "duplicate source record id: {id}"),
            Self::SourceHeadMismatch => {
                formatter.write_str("source cursor head does not match accepted records")
            }
            Self::SourceDigestMismatch => {
                formatter.write_str("source cursor digest does not match accepted records")
            }
        }
    }
}

impl std::error::Error for SourceCursorError {}

impl SourceCursorError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnsupportedSchemaVersion { .. } => "CURSOR_UNSUPPORTED_SCHEMA_VERSION",
            Self::InvalidField(..) => "CURSOR_INVALID_FIELD",
            Self::InvalidDigest(..) => "CURSOR_INVALID_DIGEST",
            Self::EmptyRecords => "CURSOR_EMPTY_RECORDS",
            Self::DuplicateRecordId(..) => "CURSOR_DUPLICATE_RECORD_ID",
            Self::SourceHeadMismatch => "CURSOR_SOURCE_HEAD_MISMATCH",
            Self::SourceDigestMismatch => "CURSOR_SOURCE_DIGEST_MISMATCH",
        }
    }
}
