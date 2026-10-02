mod delivery;
use kah_cli::{Arguments, native_run, run_pane::Pane};
use kah_run::worktree::Worktree;
use kah_sandbox::{Domain, Project};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

struct Fixture {
    source: PathBuf,
    directory: PathBuf,
    pane: Option<Pane>,
    session: Option<String>,
    overlay: Option<PathBuf>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(pane) = &self.pane {
            let _ = pane.close();
        }
        if let Some(session) = &self.session {
            let base = kah_core::run_store::RunStore::default_state_root().unwrap();
            if let Ok(Some(state)) = kah_observer::session::Store::read(&base, session)
                && state.binding.project == self.directory.join("worktree")
                && state.binding.session == *session
            {
                let _ = Command::new("codex")
                    .args(["delete", "--force", session])
                    .env_remove("CODEX_HOME")
                    .env_remove("CODEX_THREAD_ID")
                    .output();
                if let Some(run) = state.run_id
                    && let Ok(path) = kah_core::run_store::RunStore::path_for(
                        &base,
                        self.directory.join("worktree"),
                        run,
                    )
                {
                    let _ = fs::remove_dir_all(path);
                }
                let _ = fs::remove_dir_all(kah_observer::session::Store::directory(&base, session));
                use sha2::{Digest, Sha256};
                let _ = fs::remove_file(
                    base.join("session-policies")
                        .join(format!("{:x}.json", Sha256::digest(session.as_bytes()))),
                );
            }
        }
        if let Some(overlay) = &self.overlay {
            let _ = fs::remove_dir_all(overlay);
        }
        let _ = fs::remove_dir_all(&self.source);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub fn run(root: &Path) -> Value {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let name = format!("kah-pane-{}", std::process::id());
    let project = Project::select(&home.join("repos"), &home, Some((Domain::Mine, &name))).unwrap();
    assert!(project.needs_creation);
    project.initialize().unwrap();
    seed(&project.root);
    let directory = PathBuf::from(format!("/var/tmp/kah-native-pane-{}", std::process::id()));
    let mut fixture = Fixture {
        source: project.root.clone(),
        directory: directory.clone(),
        pane: None,
        session: None,
        overlay: None,
    };
    save_cleanup(root, None);
    let tree = Worktree::create(&project.root, &directory, "run-native-pane").unwrap();
    let uid = kah_run::access::service_uid().unwrap();
    for path in [&directory, root] {
        kah_run::access::traverse(path, uid).unwrap();
    }
    kah_run::access::share_tree(&tree.path, uid, true).unwrap();
    let args = Arguments {
        native: vec![
            "--model".into(),
            "gpt-6-sol".into(),
            "--no-alt-screen".into(),
        ],
        effort: Some("low".into()),
        ..Default::default()
    };
    let task = "Please use native exec_command to run: test ! -e /home/example/.codex/auth.json && echo LOGIN_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?; git status --short && echo GIT_OK; printf BASH_OK > bash-proof.txt. Then use native apply_patch to create marker.txt containing PANE_OK. Commit those two files on this disposable run branch using native Git. The external fixture controller will copy your HEAD and verify that SHA, without committing. No subagents or installs. Report and stop.";
    let command = native_run::command(&home, &tree, Domain::Mine, args, task, None).unwrap();
    fixture.overlay = command
        .get_envs()
        .find(|(key, _)| *key == "CODEX_HOME")
        .and_then(|(_, value)| value.map(PathBuf::from));
    fixture.pane = Some(Pane::start(&command).unwrap());
    let pane = fixture.pane.as_ref().unwrap();
    let native = wait(pane, &tree.path, &mut fixture.session);
    assert_eq!(
        fs::read_to_string(tree.path.join("marker.txt"))
            .unwrap()
            .trim(),
        "PANE_OK"
    );
    assert_eq!(
        fs::read_to_string(tree.path.join("bash-proof.txt")).unwrap(),
        "BASH_OK"
    );
    assert!(!project.root.join("marker.txt").exists());
    let session = fixture.session.as_ref().unwrap();
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let state = kah_observer::session::Store::read(&base, session)
        .unwrap()
        .unwrap();
    assert!(state.gap.is_none() && state.inflight.is_none());
    assert!(kah_observer::engine::projection(&state).unwrap().is_some());
    save_cleanup(root, fixture.session.as_deref());
    let delivery = delivery::prove(&home, &project.root, &tree, root, &state);
    json!({"delivery":delivery,"passed":true,"fullNativeSession":true,"paneIdentity":native["agent"],"sessionIdentity":true,"worktreeOnlyWrite":true,"observerAttached":true,"nativeSubprocessGitWorks":true})
}

fn wait(pane: &Pane, worktree: &Path, session: &mut Option<String>) -> Value {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut accepted = false;
    loop {
        let value = pane.inspect().unwrap();
        if let Some(id) = value["agent_session"]["value"].as_str() {
            *session = Some(id.into());
        }
        let output = Command::new("herdr")
            .args([
                "pane", "read", &pane.id, "--source", "recent", "--format", "text",
            ])
            .output()
            .unwrap();
        let screen = String::from_utf8_lossy(&output.stdout);
        if !accepted && screen.contains("Open restricted") {
            crate::pty::input::send(&output.stdout, "", &mut Vec::new()).unwrap();
            let sent = Command::new("herdr")
                .args(["pane", "send-keys", &pane.id, "Enter"])
                .output()
                .unwrap();
            assert!(sent.status.success());
            accepted = true;
        }
        if worktree.join("marker.txt").exists()
            && matches!(value["agent_status"].as_str(), Some("idle" | "done"))
            && session.is_some()
        {
            assert!(
                screen.contains("LOGIN_HIDDEN")
                    && screen.contains("CURL_EXIT=6")
                    && screen.contains("GIT_OK"),
                "native shell containment proof absent"
            );
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "pane did not finish: {value}; {screen}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn save_cleanup(root: &Path, session: Option<&str>) {
    fs::write(
        root.join("cleanup.json"),
        json!({"pid":std::process::id(),"session":session}).to_string(),
    )
    .unwrap();
}
pub fn cleanup_saved(root: &Path) {
    let Ok(bytes) = fs::read(root.join("cleanup.json")) else {
        return;
    };
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let pid = value["pid"].as_u64().filter(|p| *p > 0).unwrap();
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let directory = PathBuf::from(format!("/var/tmp/kah-native-pane-{pid}"));
    use sha2::{Digest, Sha256};
    let digest = format!(
        "{:x}",
        Sha256::digest(directory.join("worktree").as_os_str().as_encoded_bytes())
    );
    let fixture = Fixture {
        source: home.join(format!("repos/mine/kah-pane-{pid}")),
        directory,
        pane: None,
        session: value["session"].as_str().map(str::to_owned),
        overlay: Some(
            home.join(".local/state/kah/codex/research-offline")
                .join(&digest[..16]),
        ),
    };
    drop(fixture);
}

fn seed(project: &Path) {
    for args in [
        vec!["add", "--all"],
        vec![
            "-c",
            "user.name=Native Fixture Agent",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            "fixture source baseline",
        ],
    ] {
        assert!(
            std::process::Command::new("git")
                .arg("-C")
                .arg(project)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    }
}
