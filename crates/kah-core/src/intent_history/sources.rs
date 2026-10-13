use super::*;

/// Unverified kah transport markers are delegate context in every adapter.
/// A host-validated kah-user envelope is promoted separately by relay intake.
pub const KAH_DELEGATE_PREFIXES: &[&str] = &[
    "[kah-reviewer]",
    "[kah-builder]",
    "[kah-observer]",
    "[kah-relay]",
    "[kah-user]",
];

/// Agent notices are context, including future kah agent names. Host-validated
/// user relays promote their body through the separate receipted intake path.
#[must_use]
pub fn is_kah_delegate_message(text: &str) -> bool {
    let text = text.trim_start();
    KAH_DELEGATE_PREFIXES.iter().any(|prefix| {
        text.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    }) || text
        .strip_prefix("[kah-")
        .and_then(|rest| rest.split_once(']'))
        .is_some_and(|(name, _)| {
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// The exact event schema accepted by this history implementation.
pub const HISTORY_SCHEMA_VERSION: u32 = 2;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub fn try_new(value: impl Into<String>) -> Result<Self, HistoryError> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(HistoryError::InvalidTransition {
                        event: "id",
                        reason: "identifier must not be empty".to_owned(),
                    });
                }
                Ok(Self(value))
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

typed_id!(IntentId);
typed_id!(ConflictId);
typed_id!(EvidenceId);

/// A source provider whose records can be referred to by an intent event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceProvider {
    Codex,
    Claude,
    Bundle,
}

/// The role that produced a source record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    User,
    Delegate,
    Assistant,
    Tool,
    SystemObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceOrigin {
    Relayed,
}

/// A private, source-linked reference to an imported provider record.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceRef {
    pub provider: SourceProvider,
    /// This value is intentionally local/private and must not be included in
    /// a sanitized public export by later layers.
    pub session: String,
    pub record: String,
    pub role: SourceRole,
    pub content_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<SourceOrigin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relayer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes_receipt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_answer: Option<QuestionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_choice: Option<QuestionChoice>,
}

impl fmt::Debug for SourceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceRef")
            .field("provider", &self.provider)
            .field("record", &self.record)
            .field("role", &self.role)
            .field("content_digest", &self.content_digest)
            .finish_non_exhaustive()
    }
}

impl SourceRef {
    #[must_use]
    pub fn new(
        provider: SourceProvider,
        session: impl Into<String>,
        record: impl Into<String>,
        role: SourceRole,
        content_digest: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            session: session.into(),
            record: record.into(),
            role,
            content_digest: content_digest.into(),
            origin: None,
            relayer: None,
            supersedes_receipt: None,
            question_answer: None,
            question_choice: None,
        }
    }

    /// The session locator is private by contract; this named accessor keeps
    /// that boundary visible to callers without changing the persisted field.
    #[must_use]
    pub fn session_private(&self) -> &str {
        &self.session
    }

    pub fn validate(&self) -> Result<(), HistoryError> {
        if let Some(answer) = &self.question_answer {
            answer.validate()?;
            if self.role != SourceRole::User || self.origin != Some(SourceOrigin::Relayed) {
                return Err(HistoryError::InvalidSourceRef {
                    field: "question answer authority",
                });
            }
        }
        if let Some(choice) = &self.question_choice {
            choice.validate()?;
            if self.role != SourceRole::User
                || self.origin != Some(SourceOrigin::Relayed)
                || self.question_answer.is_some()
            {
                return Err(HistoryError::InvalidQuestionAnswer {
                    reason: "question choice must be one relayed user answer",
                });
            }
        }
        if self.supersedes_receipt.as_ref().is_some_and(|r| {
            r.is_empty()
                || self.role != SourceRole::User
                || self.origin != Some(SourceOrigin::Relayed)
        }) {
            return Err(HistoryError::InvalidSourceRef {
                field: "explicit supersedes provenance",
            });
        }
        if self.origin.is_some() != self.relayer.is_some()
            || self.origin.is_some()
                && (self.role != SourceRole::User
                    || self.relayer.as_ref().is_none_or(|s| s.is_empty()))
        {
            return Err(HistoryError::InvalidSourceRef {
                field: "relayed user provenance",
            });
        }
        non_empty(&self.session, "source session")?;
        non_empty(&self.record, "source record")?;
        non_empty(&self.content_digest, "source content digest")
    }
}

/// An explicit operator approval source.  Assistant and tool output alone
/// never carries semantic authority; it must be accompanied by this value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Approval {
    pub source_refs: Vec<SourceRef>,
}

impl Approval {
    #[must_use]
    pub fn new(source: SourceRef) -> Self {
        Self {
            source_refs: vec![source],
        }
    }

    #[must_use]
    pub fn from_sources(source_refs: Vec<SourceRef>) -> Self {
        Self { source_refs }
    }

    pub(super) fn validate(&self, event: &'static str) -> Result<(), HistoryError> {
        validate_source_refs(&self.source_refs, event)?;
        if !self.source_refs.iter().any(|source| {
            matches!(
                source.role,
                SourceRole::User | SourceRole::SystemObservation
            )
        }) {
            return Err(HistoryError::InvalidApproval { event });
        }
        Ok(())
    }
}
