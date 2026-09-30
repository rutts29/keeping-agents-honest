use kah_run::launch::{WorkerLaunch, sandbox};
use kah_sandbox::Domain;
use std::path::Path;

#[test]
fn every_unattended_domain_uses_gvisor_and_read_only_git_metadata() {
    for domain in Domain::ALL {
        let executor = sandbox(
            domain,
            Path::new("/runs/job/worktree"),
            Path::new("/runs/job/repo.git"),
            Path::new("/public/codex"),
            "fixture-image",
            42420,
            "run-test",
        )
        .unwrap();
        let args = &executor.args;
        assert!(args.windows(2).any(|p| p == ["--runtime", "runsc"]));
        assert!(args.windows(2).any(|p| p == ["--network", "none"]));
        let mounts = args
            .windows(2)
            .filter(|p| p[0] == "--mount")
            .map(|p| p[1].as_str())
            .collect::<Vec<_>>();
        assert!(
            mounts.contains(
                &"type=bind,source=/runs/job/repo.git,target=/runs/job/repo.git,readonly"
            )
        );
        assert!(mounts.contains(
            &"type=bind,source=/runs/job/worktree/.git,target=/runs/job/worktree/.git,readonly"
        ));
        assert!(!mounts.iter().any(|p| p.contains("auth.json")
            || p.contains("docker.sock")
            || p.contains("typesafe.key")));
    }
}
#[test]
fn native_exec_preserves_model_choices_and_uses_ephemeral_history() {
    let plan = WorkerLaunch {
        native: Path::new("/native/codex"),
        overlay: Path::new("/private/overlay"),
        worktree: Path::new("/runs/job/worktree"),
        model: Some("chosen-model"),
        effort: Some("chosen-effort"),
    };
    let command = plan.command();
    let args = command
        .get_args()
        .map(|a| a.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(args[0], "exec");
    assert!(args.contains(&"--ephemeral"));
    assert!(args.contains(&"--json"));
    assert!(args.windows(2).any(|p| p == ["--model", "chosen-model"]));
    assert!(args.contains(&"model_reasoning_effort=\"chosen-effort\""));
    assert!(args.contains(&"features.hooks=false"));
}
