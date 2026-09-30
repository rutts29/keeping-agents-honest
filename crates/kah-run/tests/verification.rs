use kah_run::{launch::sandbox, verification};
use kah_sandbox::Domain;
use std::{path::Path, time::Duration};
#[test]
fn verification_runs_in_the_same_confined_worktree_and_has_a_deadline() {
    let executor = sandbox(
        Domain::Mine,
        Path::new("/run/worktree"),
        Path::new("/run/repo.git"),
        Path::new("/public/codex"),
        "fixture-image",
        42420,
        "run-test",
    )
    .unwrap();
    let command = verification::command(&executor, "node --test", Duration::from_secs(30)).unwrap();
    let args = command
        .get_args()
        .map(|a| a.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(command.get_program(), "/usr/bin/timeout");
    assert!(args.contains(&"/usr/bin/docker"));
    assert!(args.windows(2).any(|a| a == ["--runtime", "runsc"]));
    assert!(args.windows(2).any(|a| a == ["--network", "none"]));
    assert_eq!(&args[args.len() - 3..], ["/bin/sh", "-lc", "node --test"]);
    assert!(!args.contains(&"exec-server"));
}
