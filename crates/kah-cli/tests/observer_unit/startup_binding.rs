use super::*;
#[test]
fn same_project_waits_for_recorded_identity_but_mismatches_stay_closed() {
    let project = Path::new("/fixture/run-owned/worktree");
    assert!(
        binding(project, project, Some(None), "worker")
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_WORKTREE_PENDING")
    );
    assert!(binding(project, project, Some(Some("worker")), "worker").is_ok());
    assert!(
        binding(project, project, Some(Some("other")), "worker")
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_WORKTREE_BINDING")
    );
    assert!(
        binding(project, Path::new("/other/worktree"), Some(None), "worker")
            .unwrap_err()
            .to_string()
            .starts_with("REVIEW_WORKTREE_BINDING")
    );
    assert!(binding(project, project, None, "worker").is_err());
}
