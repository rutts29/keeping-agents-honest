use super::*;

pub(super) fn truncate_through_completed_turn(
    thread: &mut serde_json::Map<String, Value>,
    selected_turn_id: &str,
) -> Result<(), SourceError> {
    let turns = thread
        .get_mut("turns")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            SourceError::new(
                "CODEX_THREAD_READ_RESULT_THREAD_TURNS_MUST_BE_AN_ARRAY",
                "source_turns",
                "thread/read.result.thread.turns must be an array",
            )
        })?;
    let selected_index = turns
        .iter()
        .position(|turn| turn.get("id").and_then(Value::as_str) == Some(selected_turn_id))
        .ok_or_else(|| {
            SourceError::blocked(
                "CODEX_SELECTED_CODEX_TURN_DOES_NOT_EXIST_IN_THE_SOURCE_SESSION",
                "source_turns",
                format!(
                    "selected Codex turn {selected_turn_id} does not exist in the source session"
                ),
            )
        })?;
    let location = format!("thread/read.result.thread.turns[{selected_index}]");
    let status = required_string(&turns[selected_index], "status", &location)?;
    if status == "inProgress" {
        let preceding = turns[..selected_index].iter().rev().find_map(|turn| {
            (turn.get("status").and_then(Value::as_str) == Some("completed"))
                .then(|| turn.get("id").and_then(Value::as_str))
                .flatten()
        });
        let hint = preceding.map_or_else(
            || "wait for it to complete before preparing the pair".to_owned(),
            |turn_id| format!("wait, or select preceding completed turn {turn_id}"),
        );
        return Err(SourceError::blocked(
            "CODEX_SELECTED_CODEX_TURN_IS_STILL_IN_PROGRESS",
            "source_turns",
            format!("selected Codex turn {selected_turn_id} is still in progress; {hint}"),
        ));
    }
    if status != "completed" {
        return Err(SourceError::blocked(
            "CODEX_SELECTED_CODEX_TURN_HAS_STATUS_CHOOSE_A_COMPLETED_TURN",
            "source_turns",
            format!(
                "selected Codex turn {selected_turn_id} has status {status}; choose a completed turn"
            ),
        ));
    }
    turns.truncate(selected_index + 1);
    Ok(())
}

pub(super) fn has_repository_command_evidence(
    turns: &[Value],
    repository: &Path,
) -> Result<bool, SourceError> {
    for (turn_index, turn) in turns.iter().enumerate() {
        let turn_location = format!("thread/read.result.thread.turns[{turn_index}]");
        let items = required_array(turn, "items", &turn_location)?;
        for (item_index, item) in items.iter().enumerate() {
            let item_location = format!("{turn_location}.items[{item_index}]");
            if item.get("type").and_then(Value::as_str) != Some("commandExecution") {
                continue;
            }
            let command_cwd = required_string(item, "cwd", &item_location)?;
            let command_cwd = Path::new(&command_cwd);
            if command_cwd == repository {
                return Ok(true);
            }
            if command_cwd
                .canonicalize()
                .is_ok_and(|path| path == repository)
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
