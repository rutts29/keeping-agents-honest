use super::*;

pub(super) fn binding(
    project: &Path,
    worktree: &Path,
    session: Option<Option<&str>>,
    expected: &str,
) -> Result<()> {
    if project != worktree {
        return Err("REVIEW_WORKTREE_BINDING stage=context".into());
    }
    if session == Some(None) {
        return Err(
            "REVIEW_WORKTREE_PENDING stage=context: native identity not recorded yet".into(),
        );
    }
    if session.flatten() != Some(expected) {
        return Err("REVIEW_WORKTREE_BINDING stage=context".into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "../../../tests/observer_unit/startup_binding.rs"]
mod startup_binding_tests;
