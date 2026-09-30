use kah_core::run_store::RunStore;
use kah_observer::session::Store as ObserverStore;
use kah_run::{lease::Lease, store::Store};
use std::{fs, path::PathBuf};
#[test]
#[ignore = "clean only reserved disposable kah gate fixtures after evidence; gate only"]
fn remove_finished_gate_fixtures() {
    let base = RunStore::default_state_root().unwrap();
    let runs = base.join("runs");
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let expected_parent = PathBuf::from(format!(
        "/var/tmp/kah-runs-{}",
        kah_sandbox::current_uid().unwrap()
    ));
    for entry in fs::read_dir(&runs).unwrap() {
        let entry = entry.unwrap();
        let id = entry.file_name().to_string_lossy().into_owned();
        let Ok(record) = Store::read(&runs, &id) else {
            continue;
        };
        if !fixture_name(&record.project) {
            continue;
        }
        if record.project.parent() != Some(home.join("repos/mine").as_path()) {
            continue;
        }
        if record.worktree != expected_parent.join(&id).join("worktree") {
            continue;
        }
        let Ok(_lease) = Lease::acquire(&runs, &id) else {
            continue;
        };
        assert_no_container(&id);
        if let Some(observer) = ObserverStore::read(&base, &id).unwrap()
            && let Some(ledger) = observer.run_id
        {
            let path = RunStore::path_for(&base, &record.worktree, &ledger).unwrap();
            if path.exists() {
                fs::remove_dir_all(path).unwrap();
            }
        }
        let observer = ObserverStore::directory(&base, &id);
        if observer.exists() {
            fs::remove_dir_all(observer).unwrap();
        }
        fs::remove_dir_all(record.worktree.parent().unwrap()).unwrap();
        fs::remove_dir_all(&record.project).unwrap();
        drop(_lease);
        fs::remove_dir_all(entry.path()).unwrap();
    }
}
fn fixture_name(path: &std::path::Path) -> bool {
    let name = path.file_name().unwrap().to_string_lossy();
    name.strip_prefix("kah-run-journey-")
        .or_else(|| name.strip_prefix("kah-control-journey-"))
        .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
}
fn assert_no_container(id: &str) {
    let output = std::process::Command::new("/usr/bin/docker")
        .args([
            "--host",
            "unix:///run/user/2001/docker.sock",
            "ps",
            "-aq",
            "--filter",
            &format!("label=dev.kah.run={id}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "fixture still has a container: {id}"
    );
}
