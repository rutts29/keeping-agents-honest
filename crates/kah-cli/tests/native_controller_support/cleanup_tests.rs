use super::*;
#[test]
fn absent_native_project_is_success_only_for_the_exact_fixture_path() {
    let home = std::env::var("HOME").unwrap();
    let path = Path::new("/tmp/exact-owned-fixture");
    assert!(already_purged(
        path,
        &format!(
            "No Claude Code project state found for /tmp/exact-owned-fixture under {home}/.claude.\n"
        )
    ));
    assert!(!already_purged(
        path,
        &format!(
            "No Claude Code project state found for /tmp/unrelated-project under {home}/.claude."
        )
    ));
    assert!(!already_purged(path, "Permission denied"));
}
