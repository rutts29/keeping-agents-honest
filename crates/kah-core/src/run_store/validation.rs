use super::*;

pub(super) fn validate_completion_gate_record(
    record: &CompletionGateRecord,
    path: &Path,
) -> Result<(), RunStoreError> {
    let fields = [
        record.status.as_str(),
        record.candidate_digest.as_str(),
        record.artifact_id.as_str(),
        record.command_digest.as_str(),
        record.verifier_digest.as_str(),
        record.stdout_digest.as_str(),
        record.stderr_digest.as_str(),
    ];
    let valid_status = matches!(
        record.status.as_str(),
        "passed" | "failed" | "protected_input_changed" | "candidate_changed"
    );
    if !valid_status
        || record.passed != (record.status == "passed")
        || fields.iter().any(|field| {
            field.trim().is_empty()
                || field.len() > MAX_SOURCE_CURSOR_IDENTIFIER_BYTES
                || field.chars().any(char::is_control)
        })
        || !record.candidate_digest.starts_with("sha256:")
    {
        return Err(RunStoreError::InvalidStateComponent {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Return a stable SHA-256 repository identity based on its canonical local
/// path.  The path itself is not persisted in the state layout.
pub fn repository_id(repository: impl AsRef<Path>) -> Result<String, RunStoreError> {
    let repository = repository.as_ref();
    let canonical = fs::canonicalize(repository)
        .map_err(|error| io_error("canonicalize repository", repository, error))?;
    let mut digest = Sha256::new();
    digest.update(b"driftctl.repository.v1\0");
    digest.update(canonical.as_os_str().as_encoded_bytes());
    let digest = digest.finalize();
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(super) fn validate_projection(
    history: &History,
    projection: &ActiveProjection,
) -> Result<(), RunStoreError> {
    let mut expected = project(history, projection.overflow.budget)?;
    // Resolver provenance is observational metadata, not folded intent. It
    // survives persistence without weakening any semantic field comparison.
    expected.generated_by = projection.generated_by.clone();
    if &expected == projection {
        Ok(())
    } else {
        Err(RunStoreError::ProjectionMismatch)
    }
}

/// The bytes up to and including the last newline: the committed records.
pub(super) fn committed_prefix(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(&[][..], |last| &bytes[..=last])
}

pub(super) fn parse_jsonl(input: &str) -> Result<Vec<EventRecord>, RunStoreError> {
    input
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .map_err(|error| RunStoreError::Serialization(error.to_string()))
        })
        .collect()
}

pub(super) fn validate_source_identifier(
    value: &str,
    field: &'static str,
) -> Result<(), SourceCursorError> {
    if value.trim().is_empty()
        || value.len() > MAX_SOURCE_CURSOR_IDENTIFIER_BYTES
        || value.chars().any(char::is_control)
    {
        Err(SourceCursorError::InvalidField(field))
    } else {
        Ok(())
    }
}

pub(super) fn validate_source_digest(
    value: &str,
    field: &'static str,
) -> Result<(), SourceCursorError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(SourceCursorError::InvalidDigest(field));
    };
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(SourceCursorError::InvalidDigest(field));
    }
    Ok(())
}

pub(super) fn source_cursor_digest(records: &[SourceRecordDigest]) -> String {
    let mut digest = Sha256::new();
    for record in records {
        digest.update(record.id.as_bytes());
        digest.update([0]);
        digest.update(record.content_digest.as_bytes());
        digest.update([0]);
    }
    format!("sha256:{:x}", digest.finalize())
}
