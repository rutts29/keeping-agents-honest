use kah_run::verification;
use kah_sandbox::{Domain, Executor, LaunchOptions, Project};
use std::{path::Path, time::Duration};
#[test]
fn verification_runs_in_the_same_confined_worktree_and_has_a_deadline() {
    let mut executor = Executor::new(
        &Project {
            domain: Domain::Research,
            root: "/run/worktree".into(),
            relative_cwd: Default::default(),
            needs_creation: false,
        },
        LaunchOptions::default(),
        Path::new("/public/codex"),
        "fixture-image",
        42420,
    )
    .unwrap();
    executor.use_nobody();
    let command = verification::command(&executor, "node --test", Duration::from_secs(30)).unwrap();
    let args = command
        .get_args()
        .map(|a| a.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(command.get_program(), "/usr/bin/timeout");
    assert!(args.contains(&"/usr/bin/docker"));
    assert!(args.windows(2).any(|a| a == ["--runtime", "runsc"]));
    assert!(args.windows(2).any(|a| a == ["--network", "none"]));
    assert_eq!(
        &args[args.len() - 3..],
        ["/bin/sh", "-lc", "umask 0002; node --test"]
    );
    assert!(!args.contains(&"exec-server"));
}
