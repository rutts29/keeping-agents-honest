use kah_core::{run_store::RunStore, unattended::RunStatus};
use kah_run::store::Store;
use kah_sandbox::{Domain, Project};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[test]
#[ignore = "complete native unattended CLI, gVisor, observer and Jev; gate only"]
fn a_real_task_finishes_on_a_reviewable_branch() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("kah-run-journey-{nonce}");
    let project = Project::select(&home.join("repos"), &home, Some((Domain::Mine, &name))).unwrap();
    project.initialize().unwrap();
    fs::write(
        project.root.join("greet.c"),
        "#include <stdio.h>\nint main(void) { puts(\"ALPHA\"); return 0; }\n",
    )
    .unwrap();
    git(&project.root, &["add", "greet.c"]);
    git(&project.root, &["commit", "--quiet", "-m", "fixture"]);
    let head = git(&project.root, &["rev-parse", "HEAD"]);
    let output = home.join(format!(".cache/{name}.log"));
    let file = fs::File::create(&output).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kah"))
        .current_dir(&project.root)
        .args(["run", "--model", "gpt-6-sol", "--effort", "low", "--time-limit", "180", "--test", "set -e; cc -Wall -Werror greet.c -o ./kah-greet-check; ./kah-greet-check > /tmp/kah-actual; printf 'BETA\\n' > /tmp/kah-expected; cmp /tmp/kah-expected /tmp/kah-actual; rm ./kah-greet-check", "Update greet.c so the executable prints exactly BETA followed by a newline. Use only the C standard library, without adding external dependencies. The delivered Git branch must change only greet.c. Do not spawn native subagents."])
        .stdout(Stdio::from(file.try_clone().unwrap())).stderr(Stdio::from(file))
        .spawn().unwrap();
    let exit = wait(&mut child, &output);
    let text = fs::read_to_string(&output).unwrap();
    assert!(exit.success(), "run failed; {}\n{}", output.display(), text);
    let id = text
        .lines()
        .find_map(|l| l.strip_prefix("Created "))
        .unwrap();
    let base = RunStore::default_state_root().unwrap().join("runs");
    let state = Store::read(&base, id).unwrap();
    assert_eq!(state.control.status(), RunStatus::Completed);
    assert_eq!(git(&project.root, &["rev-parse", "HEAD"]), head);
    assert!(
        fs::read_to_string(project.root.join("greet.c"))
            .unwrap()
            .contains("ALPHA")
    );
    assert_ne!(
        git(&project.root, &["rev-parse", &format!("kah/{id}")]),
        head
    );
    let report: Value =
        serde_json::from_slice(&fs::read(base.join(id).join("report.json")).unwrap()).unwrap();
    assert_eq!(report["tests"]["success"], true);
    assert_eq!(report["tests"]["commit"], report["checkpoint"]);
    assert!(report["observerGap"].is_null());
    assert!(
        before == fs::read(home.join(".codex/config.toml")).unwrap(),
        "global config changed"
    );
    let evidence = json!({"passed":true,"report":report,"sourceHeadUnchanged":true,"globalConfigUnchanged":true});
    fs::write(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/gates/evidence/phase-4/complete.json"),
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn wait(child: &mut std::process::Child, log: &std::path::Path) -> std::process::ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(300);
    loop {
        if let Some(exit) = child.try_wait().unwrap() {
            return exit;
        }
        if Instant::now() > deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("run timed out; {}", log.display());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
fn git(root: &std::path::Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
