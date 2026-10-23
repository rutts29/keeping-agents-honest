use kah_sandbox::{Domain, Executor, LaunchOptions, Project};
use std::path::{Path, PathBuf};

fn project(domain: Domain) -> Project {
    Project {
        domain,
        root: PathBuf::from("/repos/research/demo"),
        relative_cwd: PathBuf::from("src"),
        needs_creation: false,
    }
}

#[test]
fn remote_executor_exposes_only_project_and_public_binary_with_no_local_fallback() {
    let executor = Executor::new(
        &project(Domain::Research),
        LaunchOptions::default(),
        Path::new("/var/tmp/public-codex"),
        "sha256:image",
        42420,
    )
    .unwrap();
    let args = executor.args;
    for pair in [
        ["--runtime", "runsc"],
        ["--network", "none"],
        ["--cap-drop", "ALL"],
        ["--env", "HOME=/home/sandbox"],
        ["--workdir", "/repos/research/demo/src"],
    ] {
        assert!(args.windows(2).any(|w| w == pair), "missing {pair:?}");
    }
    let mounts: Vec<_> = args
        .windows(2)
        .filter(|w| w[0] == "--mount")
        .map(|w| &w[1])
        .collect();
    assert_eq!(mounts.len(), 2);
    assert!(
        mounts
            .iter()
            .any(|m| m.as_str()
                == "type=bind,source=/repos/research/demo,target=/repos/research/demo")
    );
    assert!(
        mounts
            .iter()
            .any(|m| m.ends_with("target=/opt/codex,readonly"))
    );
    assert!(
        !args
            .iter()
            .any(|a| a.contains("auth.json") || a.contains("SSH_AUTH_SOCK"))
    );
    assert!(args.ends_with(&[
        "/opt/codex".into(),
        "exec-server".into(),
        "--listen".into(),
        "stdio".into()
    ]));
}

#[test]
fn online_is_explicit_and_invalid_mount_syntax_is_refused() {
    let binary = Path::new("/var/tmp/public-codex");
    let options = LaunchOptions {
        online: true,
        yolo: false,
    };
    let untrusted =
        Executor::new(&project(Domain::Untrusted), options, binary, "image", 42420).unwrap();
    assert!(
        untrusted
            .args
            .windows(2)
            .any(|args| args == ["--network", "bridge"])
    );
    let executor =
        Executor::new(&project(Domain::Research), options, binary, "image", 42420).unwrap();
    assert!(
        executor
            .args
            .windows(2)
            .any(|w| w == ["--network", "bridge"])
    );
    let mut invalid = project(Domain::Research);
    invalid.root = PathBuf::from("/repo,readonly/escape");
    assert_eq!(
        Executor::new(&invalid, LaunchOptions::default(), binary, "image", 42420)
            .unwrap_err()
            .code,
        "MOUNT_PATH"
    );
}

#[test]
fn observer_workspace_mount_can_be_read_only() {
    let executor = Executor::new(
        &project(Domain::Research),
        LaunchOptions::default(),
        Path::new("/var/tmp/public-codex"),
        "image",
        42420,
    )
    .unwrap()
    .read_only_project();
    let mounts: Vec<_> = executor
        .args
        .windows(2)
        .filter(|p| p[0] == "--mount")
        .map(|p| &p[1])
        .collect();
    assert_eq!(mounts.len(), 2);
    assert!(mounts.iter().all(|mount| mount.ends_with(",readonly")));
}

#[test]
fn readonly_parent_promotes_nested_binds_without_duplicate_targets() {
    let p = project(Domain::Research);
    let mut executor = Executor::new(
        &p,
        LaunchOptions::default(),
        Path::new("/fixture/codex"),
        "image",
        42420,
    )
    .unwrap();
    let admin = p.root.join(".git");
    let environment = format!("target={}", p.root.display());
    executor.insert_options(["--env".into(), environment.clone()]);
    executor.add_writable_mount(&admin, &admin).unwrap();
    let executor = executor
        .with_read_only_mounts(&[(p.root.clone(), p.root.clone())])
        .unwrap();
    let mounts = executor
        .args
        .windows(2)
        .filter(|p| p[0] == "--mount")
        .map(|p| &p[1])
        .collect::<Vec<_>>();
    assert_eq!(mounts.len(), 3);
    assert!(mounts.iter().all(|m| m.ends_with(",readonly")));
    assert!(executor.args.contains(&environment));
}

#[test]
fn only_the_runsc_sandbox_start_failure_is_a_start_failure() {
    use kah_sandbox::sandbox_start_failed;
    // Docker reports a runsc sandbox-start failure.
    let start = "docker: Error response from daemon: failed to create task for container: failed to create shim task: OCI runtime create failed: creating container: cannot create sandbox: cannot read client sync file: waiting for sandbox to start: EOF\n\nRun 'docker run --help' for more information\n";
    assert!(sandbox_start_failed(start));
    assert!(!sandbox_start_failed(""));
    assert!(!sandbox_start_failed("fatal: not a git repository"));
    assert!(!sandbox_start_failed(
        "docker: Error response from daemon: OCI runtime create failed: creating container: invalid mount config"
    ));
    assert!(!sandbox_start_failed(
        "docker: Error response from daemon: Conflict. The container name is already in use"
    ));
}

#[test]
fn a_start_failure_is_retried_once_and_other_failures_are_not() {
    use kah_sandbox::output_retrying_start;
    let dir = std::env::temp_dir().join(format!("kah-start-retry-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let count = dir.join("count");
    let start = "OCI runtime create failed: creating container: cannot create sandbox: cannot read client sync file: waiting for sandbox to start: EOF";
    // Fails with the start error on the first call only.
    let script = format!(
        "n=$(cat {c} 2>/dev/null || echo 0); n=$((n+1)); echo $n > {c}; if [ $n = 1 ]; then echo '{start}' >&2; exit 125; fi; echo ok",
        c = count.display()
    );
    let output =
        output_retrying_start(std::process::Command::new("sh").args(["-c", &script])).unwrap();
    assert!(output.status.success());
    assert_eq!(std::fs::read_to_string(&count).unwrap().trim(), "2");
    // Any other failure is returned at once.
    std::fs::remove_file(&count).unwrap();
    let other = format!(
        "echo x >> {c}; echo 'permission denied' >&2; exit 1",
        c = count.display()
    );
    let output =
        output_retrying_start(std::process::Command::new("sh").args(["-c", &other])).unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 1);
    // A second start failure is returned, not retried again.
    std::fs::remove_file(&count).unwrap();
    let always = format!(
        "echo x >> {c}; echo '{start}' >&2; exit 125",
        c = count.display()
    );
    let output =
        output_retrying_start(std::process::Command::new("sh").args(["-c", &always])).unwrap();
    assert!(!output.status.success());
    assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 2);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Doctests run binaries from TMPDIR, so /tmp alone allows exec. Docker's
/// tmpfs default is noexec; the private home keeps that default.
#[test]
fn only_tmp_allows_exec_among_the_sandbox_tmpfs_mounts() {
    let mut executor = Executor::new(
        &project(Domain::Research),
        LaunchOptions::default(),
        Path::new("/var/tmp/public-codex"),
        "sha256:image",
        42420,
    )
    .unwrap();
    executor.use_nobody();
    let tmpfs: Vec<_> = executor
        .args
        .windows(2)
        .filter(|w| w[0] == "--tmpfs")
        .map(|w| w[1].as_str())
        .collect();
    assert_eq!(
        tmpfs,
        [
            "/tmp:rw,exec,nosuid,nodev,size=512m,mode=1777",
            "/run:rw,nosuid,nodev,size=64m,mode=755",
            "/home/sandbox:rw,nosuid,nodev,size=512m,mode=700,uid=65534,gid=65534",
        ]
    );
}
