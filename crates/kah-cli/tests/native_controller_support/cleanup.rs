use super::*;
use kah_observer::session;
use sha2::{Digest, Sha256};
#[path = "cleanup_tests.rs"]
mod tests;

pub fn cleanup(root: &Path) {
    let Ok(bytes) = fs::read(root.join("cleanup.json")) else {
        return;
    };
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    let id = saved["run"].as_str().unwrap();
    let source = PathBuf::from(saved["source"].as_str().unwrap());
    let base = RunStore::default_state_root().unwrap();
    let record = Store::read(&base.join("runs"), id).unwrap();
    assert_eq!(record.project, source);
    assert!(
        source
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("kah-native-run-")
    );
    assert_eq!(record.worktree.parent().unwrap().file_name().unwrap(), id);
    kah_cli::native_child::stop_executors(&record.worktree).unwrap();
    if let Some(children) = saved["children"].as_array() {
        assert_eq!(record.native.as_ref().unwrap().provider, "codex");
        for child in children {
            let id = child.as_str().unwrap();
            assert_ne!(Some(id), record.native.as_ref().unwrap().session.as_deref());
            cleanup_session(&base, &record, id);
        }
    }
    cleanup_native(&base, &record);
    if saved["extraClaudeProject"] == true {
        assert_eq!(record.native.as_ref().unwrap().provider, "claude");
        purge(&record.project);
    }
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let hash = format!(
        "{:x}",
        Sha256::digest(record.worktree.as_os_str().as_encoded_bytes())
    );
    let _ = fs::remove_dir_all(
        home.join(".local/state/kah/codex/research-offline")
            .join(&hash[..16]),
    );
    let project = Project {
        domain: Domain::Research,
        root: record.worktree.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let _ = fs::remove_dir_all(kah_cli::claude_setup::runtime_directory(&project).unwrap());
    remove_owned(&source);
    remove_owned(record.worktree.parent().unwrap());
    remove_owned(&Store::path(&base.join("runs"), id).unwrap());
}
fn cleanup_native(base: &Path, record: &Record) {
    let native = record.native.as_ref().unwrap();
    if let (Some(pane), Some(socket)) = (&native.pane, &native.herdr_socket) {
        assert!(socket.to_string_lossy().contains("kah-gate-"));
        let _ = fs::remove_file(kah_cli::relay_store::pane_path(pane, socket).unwrap());
    }
    // The harness already stopped its exact named server. Pane IDs are scoped
    // to that server and must never be closed through the caller's connection.
    if let Some(id) = &native.session {
        cleanup_session(base, record, id);
    }
    if native.provider == "claude" {
        // Only the exact fixture worktree just verified against its source/run.
        purge(&record.worktree);
    }
}
fn purge(project: &Path) {
    let output = Command::new("claude")
        .args(["purge", "--yes"])
        .arg(project)
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success() || already_purged(project, &text),
        "fixture purge failed: {text}"
    );
}

/// Claude may print notices (e.g. 2.1.302's rename notice) around the result.
fn already_purged(project: &Path, text: &str) -> bool {
    let expected = format!(
        "No Claude Code project state found for {} under {}.",
        project.display(),
        std::env::var("HOME").unwrap() + "/.claude"
    );
    text.lines().any(|line| line.trim() == expected)
}

fn remove_owned(path: &Path) {
    match fs::remove_dir_all(path) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => panic!("owned fixture cleanup failed for {path:?}: {error}"),
    }
}

fn cleanup_session(base: &Path, record: &Record, id: &str) {
    if let Some(state) = session::Store::read(base, id).unwrap() {
        assert_eq!(state.binding.project, record.worktree);
        if let Some(run) = state.run_id {
            fs::remove_dir_all(RunStore::path_for(base, &record.worktree, run).unwrap()).unwrap();
        }
    }
    let _ = fs::remove_dir_all(session::Store::directory(base, id));
    let _ = fs::remove_file(
        base.join("session-policies")
            .join(format!("{:x}.json", Sha256::digest(id.as_bytes()))),
    );
    if record.native.as_ref().unwrap().provider == "codex" {
        assert!(
            Command::new("codex")
                .args(["delete", "--force", id])
                .env_remove("CODEX_HOME")
                .env_remove("CODEX_THREAD_ID")
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn an_already_purged_project_is_recognized_among_other_notice_lines() {
    let project = Path::new("/var/tmp/kah-runs/run-1/worktree");
    let home = std::env::var("HOME").unwrap();
    let result = format!(
        "No Claude Code project state found for {} under {home}/.claude.",
        project.display()
    );
    // Claude 2.1.302 prints a rename notice first (5h-d on 8558946).
    let text = format!(
        "'claude project purge' is now 'claude purge'. The old name still works for now.\n{result}\n"
    );
    assert!(already_purged(project, &text));
    assert!(already_purged(project, &result));
    assert!(!already_purged(
        project,
        "'claude project purge' is now 'claude purge'.\n"
    ));
    assert!(!already_purged(Path::new("/other"), &text));
}
