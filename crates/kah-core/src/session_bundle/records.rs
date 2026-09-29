use super::*;

/// The private provenance shared by every record in a bundle.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct BundleSource {
    pub(super) provider: SourceProvider,
    pub(super) session_ref: String,
    pub(super) repository_digest: String,
    pub(super) head: String,
    pub(super) digest: String,
}

impl BundleSource {
    #[must_use]
    pub fn provider(&self) -> SourceProvider {
        self.provider.clone()
    }

    /// Local-only source locator. Do not include this in a sanitized export.
    #[must_use]
    pub fn session_ref_private(&self) -> &str {
        &self.session_ref
    }

    #[must_use]
    pub fn repository_digest(&self) -> &str {
        &self.repository_digest
    }

    #[must_use]
    pub fn head(&self) -> &str {
        &self.head
    }

    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

impl fmt::Debug for BundleSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BundleSource")
            .field("provider", &self.provider)
            .field("repository_digest", &self.repository_digest)
            .field("head", &self.head)
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

/// One normalized source message, retained in the provider's original order.
#[derive(Clone, Eq, PartialEq, Serialize)]
pub struct BundleRecord {
    pub(super) id: String,
    pub(super) role: SourceRole,
    pub(super) content: String,
    pub(super) content_digest: String,
}

impl BundleRecord {
    /// Creates a record and calculates its SHA-256 digest from its normalized
    /// identifier, role, and text.
    pub fn new(
        id: impl Into<String>,
        role: SourceRole,
        content: impl Into<String>,
    ) -> Result<Self, SessionBundleError> {
        let id = id.into();
        let content = normalize_content(content.into());
        validate_identifier(&id, "record id")?;
        validate_content(&content)?;
        let content_digest = record_digest(&id, &role, &content);
        Ok(Self {
            id,
            role,
            content,
            content_digest,
        })
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn role(&self) -> SourceRole {
        self.role.clone()
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub fn content_digest(&self) -> &str {
        &self.content_digest
    }

    #[must_use]
    pub fn is_authoritative(&self) -> bool {
        matches!(self.role, SourceRole::User)
    }
}

impl fmt::Debug for BundleRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BundleRecord")
            .field("id", &self.id)
            .field("role", &self.role)
            .field("content_digest", &self.content_digest)
            .finish()
    }
}
