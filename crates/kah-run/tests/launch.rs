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
        let mounted = executor
            .with_read_only_mounts(&[("/skills/source".into(), "/skills/target".into())])
            .unwrap();
        let image = mounted
            .args
            .iter()
            .position(|a| a == "fixture-image")
            .unwrap();
        assert!(
            mounted
                .args
                .iter()
                .position(|a| a == "type=bind,source=/skills/source,target=/skills/target,readonly")
                .unwrap()
                < image
        );
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
    assert!(args.windows(2).any(|p| p == ["/native/codex", "exec"]));
    assert!(args.contains(&"--ephemeral"));
    assert!(args.contains(&"--json"));
    assert!(args.windows(2).any(|p| p == ["--model", "chosen-model"]));
    assert!(args.contains(&"model_reasoning_effort=\"chosen-effort\""));
    assert!(args.contains(&"features.hooks=false"));
    assert!(args.contains(&"features.multi_agent=false"));
    assert!(args.contains(&"features.multi_agent_v2=false"));
    assert!(args.contains(&"features.goals=false"));
}

#[test]
fn native_worker_dies_when_its_controller_is_killed() {
    use std::{fs, process::Command, thread, time::Duration};
    let root = std::env::temp_dir().join(format!("kah-parent-death-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let mut parent = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "parent_death_helper", "--ignored"])
        .env("KAH_TEST_PARENT_DEATH", &root)
        .spawn()
        .unwrap();
    let pid_file = root.join("pid");
    for _ in 0..100 {
        if pid_file.exists() {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let pid = fs::read_to_string(&pid_file).expect("helper spawned native worker");
    parent.kill().unwrap();
    parent.wait().unwrap();
    thread::sleep(Duration::from_millis(150));
    let alive = fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
        s.split_once(") ")
            .is_some_and(|(_, fields)| !fields.starts_with('Z'))
    });
    let _ = Command::new("/bin/kill").args(["-KILL", &pid]).output();
    fs::remove_dir_all(root).unwrap();
    assert!(!alive, "native worker survived its controller");
}

#[test]
#[ignore = "subprocess helper; no model calls"]
fn parent_death_helper() {
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, thread, time::Duration};
    let root = PathBuf::from(std::env::var_os("KAH_TEST_PARENT_DEATH").unwrap());
    let binary = root.join("native");
    fs::write(&binary, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let launch = WorkerLaunch {
        native: &binary,
        overlay: &root,
        worktree: &root,
        model: None,
        effort: None,
    };
    let mut child = launch.command().spawn().unwrap();
    // Allow the native exec to finish before killing its parent.
    thread::sleep(Duration::from_millis(100));
    fs::write(root.join("pid"), child.id().to_string()).unwrap();
    child.wait().unwrap();
}
