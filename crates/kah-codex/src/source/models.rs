use super::*;

#[derive(Clone, Eq, PartialEq)]
pub struct ImportedSession {
    pub(super) session_id: String,
    pub(super) repository_digest: String,
    pub(super) native_goal: NativeGoal,
    pub(super) thread_snapshot: Value,
    pub(super) records: Vec<ImportedRecord>,
    pub(super) allow_ancestor_cwd: bool,
    pub(super) through_turn_id: Option<String>,
}

impl ImportedSession {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn imported_user_record_count(&self) -> usize {
        self.records
            .iter()
            .filter(|record| matches!(record.role, SourceRole::User))
            .count()
    }

    pub fn redacted_session(&self) -> String {
        let digest = Sha256::digest(self.session_id.as_bytes());
        let opaque: String = digest[..6]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        format!("codex-session:sha256:{opaque}")
    }

    pub fn source_digest(&self) -> String {
        self.neutral_bundle()
            .expect("parsed Codex user records must form a valid neutral bundle")
            .source()
            .digest()
            .to_owned()
    }

    pub fn references_checkout_path_in_content(
        &self,
        repository: &Path,
    ) -> Result<bool, SourceError> {
        if self
            .native_goal
            .text()
            .is_some_and(|goal| text_references_checkout(goal, repository))
        {
            return Ok(true);
        }
        let turns = required_array(&self.thread_snapshot, "turns", "thread/read.result.thread")?;
        Ok(turns.iter().any(|turn| {
            turn.get("items")
                .and_then(Value::as_array)
                .is_some_and(|items| {
                    items
                        .iter()
                        .any(|item| provider_item_references_checkout(item, repository))
                })
        }))
    }

    /// Converts the App Server's explicit user messages to strict neutral
    /// records before any semantic resolver can inspect them.
    pub fn neutral_bundle(&self) -> Result<NeutralSessionBundle, SourceError> {
        let records = self
            .records
            .iter()
            .map(|record| BundleRecord::new(&record.id, record.role.clone(), &record.content))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                SourceError::new(
                    "CODEX_INVALID_IMPORTED_CODEX_RECORD",
                    "source_models",
                    format!("invalid imported Codex record: {error}"),
                )
            })?;
        NeutralSessionBundle::from_records_with_native_goal(
            SourceProvider::Codex,
            &self.session_id,
            &self.repository_digest,
            self.native_goal.clone(),
            records,
        )
        .map_err(|error| {
            SourceError::new(
                "CODEX_INVALID_IMPORTED_CODEX_BUNDLE",
                "source_models",
                format!("invalid imported Codex bundle: {error}"),
            )
        })
    }
}

impl fmt::Debug for ImportedSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportedSession")
            .field("redacted_session", &self.redacted_session())
            .field("repository_digest", &self.repository_digest)
            .field("record_count", &self.records.len())
            .field("user_record_count", &self.imported_user_record_count())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct ImportedRecord {
    pub(super) id: String,
    pub(super) role: SourceRole,
    pub(super) content: String,
}

impl fmt::Debug for ImportedRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImportedRecord")
            .field("id", &self.id)
            .field("role", &self.role)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionSelection<'a> {
    Last,
    Explicit {
        id: &'a str,
        allow_ancestor_cwd: bool,
    },
}
