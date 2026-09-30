use super::*;

pub(super) fn validate_records(records: &[BundleRecord]) -> Result<(), SessionBundleError> {
    if records.is_empty() {
        return Err(SessionBundleError::EmptyRecords);
    }
    let mut ids = BTreeSet::new();
    for record in records {
        if !ids.insert(&record.id) {
            return Err(SessionBundleError::DuplicateRecordId(record.id.clone()));
        }
    }
    Ok(())
}

pub(super) fn validate_identifier(
    value: &str,
    field: &'static str,
) -> Result<(), SessionBundleError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(SessionBundleError::InvalidField(field));
    }
    Ok(())
}

pub(super) fn validate_content(value: &str) -> Result<(), SessionBundleError> {
    if value.is_empty() || value.len() > MAX_CONTENT_BYTES || value.contains('\0') {
        return Err(SessionBundleError::InvalidField("record content"));
    }
    Ok(())
}

pub(super) fn validate_native_goal_text(value: &str) -> Result<(), SessionBundleError> {
    if value.is_empty() || value.len() > MAX_CONTENT_BYTES || value.contains('\0') {
        return Err(SessionBundleError::InvalidField("native_goal text"));
    }
    Ok(())
}

pub(super) fn validate_sha256(value: &str, field: &'static str) -> Result<(), SessionBundleError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(SessionBundleError::InvalidDigest(field));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SessionBundleError::InvalidDigest(field));
    }
    Ok(())
}

pub(super) fn record_digest(id: &str, role: &SourceRole, content: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(id.as_bytes());
    digest.update([0]);
    digest.update(role_name(role).as_bytes());
    digest.update([0]);
    digest.update(content.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

pub(super) fn source_digest(records: &[BundleRecord]) -> String {
    let mut digest = Sha256::new();
    for record in records {
        digest.update(record.id.as_bytes());
        digest.update([0]);
        digest.update(record.content_digest.as_bytes());
        digest.update([0]);
    }
    format!("sha256:{:x}", digest.finalize())
}

pub(super) fn role_name(role: &SourceRole) -> &'static str {
    match role {
        SourceRole::User => "user",
        SourceRole::Assistant => "assistant",
        SourceRole::Tool => "tool",
        SourceRole::SystemObservation => "system_observation",
    }
}
