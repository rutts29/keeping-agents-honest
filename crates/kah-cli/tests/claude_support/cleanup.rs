//! Native purge may only target the exact, nonsymlink project this fixture created.
use std::{fs, path::Path, process::Command};

pub(super) fn command(project: &Path, created: &Path) -> Result<Command, String> {
    if project != created
        || !project
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("kah-claude-live-"))
        || !fs::symlink_metadata(project).is_ok_and(|m| m.is_dir())
        || project.canonicalize().ok().as_deref() != Some(created)
    {
        return Err(
            "CLAUDE_FIXTURE_SCOPE stage=cleanup: refusing purge outside the exact created project"
                .into(),
        );
    }
    let mut command = Command::new("claude");
    command.args(["purge", "--yes"]).arg(project);
    Ok(command)
}

#[test]
fn native_purge_accepts_only_the_exact_created_fixture_path() {
    let base = std::env::temp_dir().join(format!(
        "kah-purge-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let fixture = base.join("kah-claude-live-owned");
    let user = base.join("user-project");
    fs::create_dir_all(&fixture).unwrap();
    fs::create_dir(&user).unwrap();
    let native = command(&fixture, &fixture).unwrap();
    assert_eq!(native.get_args().last().unwrap(), fixture.as_os_str());
    // 'claude purge --yes <path>' (2.1.302 renamed 'claude project purge').
    assert_eq!(
        native.get_args().collect::<Vec<_>>(),
        ["purge", "--yes", fixture.as_os_str().to_str().unwrap()]
    );
    assert!(command(&user, &fixture).is_err());
    assert!(command(&user, &user).is_err());
    fs::remove_dir(&fixture).unwrap();
    std::os::unix::fs::symlink(&user, &fixture).unwrap();
    assert!(command(&fixture, &fixture).is_err());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn rejected_purge_also_leaves_the_unowned_directory_untouched() {
    let base = std::env::temp_dir().join(format!(
        "kah-cleanup-refusal-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let created = base.join("kah-claude-live-owned");
    let user = base.join("user-project");
    fs::create_dir_all(&created).unwrap();
    fs::create_dir(&user).unwrap();
    fs::write(user.join("keep.txt"), "untouched").unwrap();
    let fixture = super::Fixture {
        root: user.clone(),
        created_root: created,
        id: "fixture-only".into(),
        base: base.join("state"),
        evidence: base.clone(),
        transcript_directory: base.join("transcripts"),
    };
    drop(fixture);
    assert_eq!(
        fs::read_to_string(user.join("keep.txt")).unwrap(),
        "untouched"
    );
    fs::remove_dir_all(base).unwrap();
}
