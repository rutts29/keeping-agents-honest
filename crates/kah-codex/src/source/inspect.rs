use super::*;

pub fn inspect(
    root: &Path,
    selection: SessionSelection<'_>,
) -> Result<ImportedSession, SourceError> {
    inspect_at(root, selection, None)
}

pub fn inspect_through_turn(
    root: &Path,
    selection: SessionSelection<'_>,
    through_turn_id: &str,
) -> Result<ImportedSession, SourceError> {
    let through_turn_id = validate_explicit_turn_id(through_turn_id)?;
    inspect_at(root, selection, Some(&through_turn_id))
}

pub(super) fn inspect_at(
    root: &Path,
    selection: SessionSelection<'_>,
    through_turn_id: Option<&str>,
) -> Result<ImportedSession, SourceError> {
    let canonical_root = root.canonicalize().map_err(|error| {
        SourceError::new(
            "CODEX_COULD_NOT_CANONICALIZE_CURRENT_DIRECTORY",
            "source_inspect",
            format!("could not canonicalize current directory: {error}"),
        )
    })?;
    let canonical_root = canonical_root.to_str().ok_or_else(|| {
        SourceError::new(
            "CODEX_CURRENT_DIRECTORY_IS_NOT_VALID_UTF_8",
            "source_inspect",
            "current directory is not valid UTF-8",
        )
    })?;
    let mut server = AppServer::start()?;
    let result = (|| {
        server.initialize()?;
        let (selected_id, allow_ancestor_cwd) = match selection {
            SessionSelection::Last => (server.latest_thread_id(canonical_root)?, false),
            SessionSelection::Explicit {
                id,
                allow_ancestor_cwd,
            } => (validate_explicit_session_id(id)?, allow_ancestor_cwd),
        };
        let thread = server.read_thread(&selected_id)?;
        let native_goal = server.observe_goal(&selected_id)?;
        let imported = parse_imported_session(
            thread,
            canonical_root,
            &selected_id,
            native_goal,
            allow_ancestor_cwd,
            through_turn_id,
        )?;
        // Convert at the provider boundary. This validates the exact neutral
        // handoff without classifying intent or changing public inspect output.
        imported.neutral_bundle()?;
        Ok(imported)
    })();
    server.stop();
    result
}

/// Re-import the exact selected parent after a paid resolver call and reject
/// a stale result without exposing the private session locator.
pub fn verify_unchanged(root: &Path, expected: &ImportedSession) -> Result<(), SourceError> {
    let selection = SessionSelection::Explicit {
        id: &expected.session_id,
        allow_ancestor_cwd: expected.allow_ancestor_cwd,
    };
    let observed = match expected.through_turn_id.as_deref() {
        Some(turn_id) => inspect_through_turn(root, selection, turn_id)?,
        None => inspect(root, selection)?,
    };
    if &observed == expected {
        Ok(())
    } else {
        Err(SourceError::new(
            "CODEX_SOURCE_SESSION_OR_NATIVE_GOAL_CHANGED_DURING_INSPECT",
            "source_inspect",
            "Codex source session or native goal changed during inspect",
        ))
    }
}

pub(super) fn validate_explicit_session_id(id: &str) -> Result<String, SourceError> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(SourceError::new(
            "CODEX_INVALID_EXPLICIT_CODEX_SESSION_ID",
            "source_inspect",
            "invalid explicit Codex session ID",
        ));
    }
    Ok(id.to_owned())
}

pub(super) fn validate_explicit_turn_id(id: &str) -> Result<String, SourceError> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(SourceError::new(
            "CODEX_INVALID_EXPLICIT_CODEX_TURN_ID",
            "source_inspect",
            "invalid explicit Codex turn ID",
        ));
    }
    Ok(id.to_owned())
}
