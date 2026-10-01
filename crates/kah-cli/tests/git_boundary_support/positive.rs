use super::{git, save};
use crate::pty::{Terminal, quote};
use serde_json::json;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    time::{Duration, Instant},
};

#[test]
#[ignore = "positive control: the Git command canaries really execute on the host"]
fn git_command_traps_fire_without_control_mounts() {
    let root = std::env::temp_dir().join(format!("kah-git-canary-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "--quiet", "--template="]);
    git(&root, &["config", "user.name", "kah fixture"]);
    git(&root, &["config", "user.email", "kah@localhost"]);
    git(&root, &["config", "commit.gpgsign", "false"]);
    git(
        &root,
        &[
            "config",
            "core.fsmonitor",
            "printf FSMONITOR >> HOST_CANARY",
        ],
    );
    git(
        &root,
        &["config", "core.pager", "printf PAGER >> HOST_CANARY"],
    );
    fs::create_dir_all(root.join(".git/hooks")).unwrap();
    fs::write(
        root.join(".git/hooks/pre-commit"),
        "#!/bin/sh\nprintf HOOK >> HOST_CANARY\n",
    )
    .unwrap();
    fs::set_permissions(
        root.join(".git/hooks/pre-commit"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    fs::write(root.join("file"), "fixture").unwrap();
    git(&root, &["add", "file"]);
    git(&root, &["commit", "--quiet", "-m", "fixture"]);
    git(&root, &["status", "--porcelain"]);
    let command = format!(
        "cd {} && env -u GIT_PAGER git --paginate log -1; printf DONE > {}",
        quote(root.to_str().unwrap()),
        quote(root.join("finished").to_str().unwrap())
    );
    let mut terminal = Terminal::start(&command, &root.join("pager.log"));
    let deadline = Instant::now() + Duration::from_secs(15);
    while !root.join("finished").exists() {
        terminal.respond_to_cursor_queries();
        assert!(Instant::now() < deadline, "pager control did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(terminal);
    let canary = fs::read_to_string(root.join("HOST_CANARY")).unwrap();
    for name in ["FSMONITOR", "PAGER", "HOOK"] {
        assert!(canary.contains(name), "{name} trap did not fire");
    }
    fs::remove_dir_all(root).unwrap();
    save(
        "git-controls-positive.json",
        json!({"hostFsmonitorFired":true,"hostPagerFired":true,"hostCommitHookFired":true,"passed":true}),
    );
}
