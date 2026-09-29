use super::*;

pub(super) fn parse_imported_session(
    mut response: Value,
    canonical_root: &str,
    requested_id: &str,
    native_goal: NativeGoal,
    allow_ancestor_cwd: bool,
    through_turn_id: Option<&str>,
) -> Result<ImportedSession, SourceError> {
    let thread = response
        .get_mut("thread")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_THREAD_READ_RESULT_THREAD_IS_MISSING",
                "source_session",
                "thread/read.result.thread is missing",
            )
        })?;
    let id = required_object_string(thread, "id", "thread/read.result.thread")?;
    if id != requested_id {
        return Err(SourceError::new(
            "CODEX_THREAD_READ_RETURNED_A_DIFFERENT_SESSION",
            "source_session",
            "thread/read returned a different session",
        ));
    }
    if let Some(through_turn_id) = through_turn_id {
        truncate_through_completed_turn(thread, through_turn_id)?;
    }
    let thread = response.get("thread").ok_or_else(|| {
        SourceError::new(
            "CODEX_THREAD_READ_RESULT_THREAD_IS_MISSING",
            "source_session",
            "thread/read.result.thread is missing",
        )
    })?;
    let cwd = required_string(thread, "cwd", "thread/read.result.thread")?;
    let turns = required_array(thread, "turns", "thread/read.result.thread")?;
    validate_repository(&cwd, canonical_root, turns, allow_ancestor_cwd)?;
    let records = parse_records(turns)?;
    Ok(ImportedSession {
        session_id: id,
        repository_digest: repository_digest(canonical_root),
        native_goal,
        thread_snapshot: thread.clone(),
        records,
        allow_ancestor_cwd,
        through_turn_id: through_turn_id.map(str::to_owned),
    })
}

pub(super) fn required_object_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
    location: &str,
) -> Result<String, SourceError> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .map(str::to_owned)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_MUST_BE_A_NON_EMPTY_STRING",
                "source_session",
                format!("{location}.{field} must be a non-empty string"),
            )
        })
}

fn parse_records(turns: &[Value]) -> Result<Vec<ImportedRecord>, SourceError> {
    let mut records = Vec::new();
    for (turn_index, turn) in turns.iter().enumerate() {
        let turn_location = format!("thread/read.result.thread.turns[{turn_index}]");
        let items = required_array(turn, "items", &turn_location)?;
        for (item_index, item) in items.iter().enumerate() {
            let item_location = format!("{turn_location}.items[{item_index}]");
            let provider_type = required_string(item, "type", &item_location)?;
            if provider_type == "userMessage" {
                parse_user_message(item, &item_location, &mut records)?;
                continue;
            }
            let role = provider_item_role(&provider_type).ok_or_else(|| {
                SourceError::new(
                    "CODEX_TYPE_IS_AN_UNSUPPORTED_CODEX_THREAD_ITEM",
                    "source_session",
                    format!("{item_location}.type is an unsupported Codex thread item"),
                )
            })?;
            validate_provider_item(item, &provider_type, &item_location)?;
            let id = required_string(item, "id", &item_location)?;
            records.push(ImportedRecord {
                id,
                role,
                content: opaque_provider_evidence(item, &provider_type)?,
            });
        }
    }
    Ok(records)
}

fn validate_repository(
    cwd: &str,
    canonical_root: &str,
    turns: &[Value],
    allow_ancestor_cwd: bool,
) -> Result<(), SourceError> {
    let repository = Path::new(canonical_root);
    if Path::new(cwd).starts_with(repository)
        && Path::new(cwd)
            .canonicalize()
            .is_ok_and(|path| path.starts_with(repository))
    {
        return Ok(());
    }
    if cwd != canonical_root {
        if !allow_ancestor_cwd {
            return Err(SourceError::new(
                "CODEX_SESSION_DOES_NOT_BELONG_TO_THE_CURRENT_REPOSITORY_EXPLICIT_ANCESTOR",
                "source_session",
                "Codex session does not belong to the current repository; explicit ancestor binding requires --allow-ancestor-cwd",
            ));
        }
        let repository = Path::new(canonical_root);
        let session_cwd = Path::new(cwd).canonicalize().map_err(|_| {
            SourceError::new(
                "CODEX_SESSION_DOES_NOT_BELONG_TO_THE_CURRENT_REPOSITORY",
                "source_session",
                "Codex session does not belong to the current repository",
            )
        })?;
        if !repository.starts_with(&session_cwd) {
            return Err(SourceError::new(
                "CODEX_SESSION_DOES_NOT_BELONG_TO_THE_CURRENT_REPOSITORY",
                "source_session",
                "Codex session does not belong to the current repository",
            ));
        }
        if !has_repository_command_evidence(turns, repository)? {
            return Err(SourceError::new(
                "CODEX_SESSION_DOES_NOT_HAVE_COMMAND_EVIDENCE_FOR_THE_CURRENT_REPOSITORY",
                "source_session",
                "Codex session does not have command evidence for the current repository",
            ));
        }
    }
    Ok(())
}
