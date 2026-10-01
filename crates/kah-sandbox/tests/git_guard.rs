use kah_sandbox::{Domain, Executor, LaunchOptions, Project, git_guard};
use std::{fs, path::Path, process::Command};

#[test]
fn protected_git_controls_keep_host_git_operations_native() {
    let root = std::env::temp_dir().join(format!("kah-git-controls-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "--quiet", "--template="]);
    let project = Project {
        domain: Domain::Research,
        root: root.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let executor = Executor::new(
        &project,
        LaunchOptions::default(),
        Path::new("/fixture/codex"),
        "fixture:1",
        42420,
    )
    .unwrap();
    let executor = git_guard::protect(executor, &root, None).unwrap();
    for name in ["config", "config.worktree", "commondir", "hooks"] {
        assert!(executor.args.iter().any(|arg| arg
            == &format!(
                "type=bind,source={},target={},readonly",
                root.join(".git").join(name).display(),
                root.join(".git").join(name).display()
            )));
    }
    fs::write(root.join("file"), "content").unwrap();
    git(&root, &["add", "file"]);
    git(
        &root,
        &[
            "-c",
            "user.name=kah test",
            "-c",
            "user.email=kah@localhost",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    git(&root, &["branch", "second"]);
    git(&root, &["status", "--porcelain"]);
    git(&root, &["log", "-1", "--oneline"]);
    fs::remove_dir_all(root).unwrap();
}

fn git(root: &Path, args: &[&str]) {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn linked_admin_is_pinned_once_and_aliased_controls_are_rejected() {
    let root = std::env::temp_dir().join(format!("kah-git-linked-{}", std::process::id()));
    let project = root.join("tree");
    let admin = root.join("admin");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir(&admin).unwrap();
    fs::write(
        project.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    let make = || {
        Executor::new(
            &Project {
                domain: Domain::Research,
                root: project.clone(),
                relative_cwd: Default::default(),
                needs_creation: false,
            },
            LaunchOptions::default(),
            Path::new("/fixture/codex"),
            "fixture:1",
            42420,
        )
        .unwrap()
    };
    assert!(git_guard::protect(make(), &project, None).is_err());
    let mut executor = make();
    executor.add_writable_mount(&admin, &admin).unwrap();
    let protected = git_guard::protect(executor, &project, Some(&admin)).unwrap();
    let bind = format!(
        "type=bind,source={},target={}",
        admin.display(),
        admin.display()
    );
    assert_eq!(protected.args.iter().filter(|s| *s == &bind).count(), 1);
    fs::hard_link(admin.join("config"), root.join("alias")).unwrap();
    assert!(git_guard::protect(make(), &project, Some(&admin)).is_err());
    fs::remove_dir_all(root).unwrap();
}
