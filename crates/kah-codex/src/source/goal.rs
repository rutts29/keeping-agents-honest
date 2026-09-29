use super::*;

pub(super) fn parse_native_goal(
    response: Value,
    requested_id: &str,
) -> Result<NativeGoal, SourceError> {
    let Some(goal) = response.get("goal") else {
        return Err(SourceError::new(
            "CODEX_THREAD_GOAL_GET_RESULT_GOAL_IS_MISSING",
            "source_goal",
            "thread/goal/get.result.goal is missing",
        ));
    };
    if goal.is_null() {
        return Ok(NativeGoal::Absent);
    }
    let thread_id = required_string(goal, "threadId", "thread/goal/get.result.goal")?;
    if thread_id != requested_id {
        return Err(SourceError::new(
            "CODEX_THREAD_GOAL_GET_RETURNED_A_GOAL_FOR_A_DIFFERENT_SESSION",
            "source_goal",
            "thread/goal/get returned a goal for a different session",
        ));
    }
    let objective = required_text(goal, "objective", "thread/goal/get.result.goal")?;
    NativeGoal::known(objective).map_err(|error| {
        SourceError::new(
            "CODEX_INVALID_CODEX_NATIVE_GOAL",
            "source_goal",
            format!("invalid Codex native goal: {error}"),
        )
    })
}

pub(super) fn repository_digest(canonical_root: &str) -> String {
    let digest = Sha256::digest(canonical_root.as_bytes());
    format!("sha256:{digest:x}")
}
