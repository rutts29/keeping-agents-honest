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
    assert!(Executor::new(&project(Domain::Untrusted), options, binary, "image", 42420).is_err());
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
