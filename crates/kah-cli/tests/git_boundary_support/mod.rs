use crate::{claude_support::Fixture, history, pty};
mod positive;
use kah_cli::{Arguments, Launch};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn prepare(root: &Path) {
    git(root, &["config", "user.name", "kah fixture"]);
    git(root, &["config", "user.email", "kah@localhost"]);
    git(root, &["config", "commit.gpgsign", "false"]);
    let script = r##"#!/bin/sh
attempt() { "$@" >/dev/null 2>&1; printf '%s=%s\n' "$1" "$?" >> attempts.txt; }
attempt git config core.fsmonitor 'printf ESCAPED > HOST_CANARY'
attempt git config core.pager 'printf ESCAPED > HOST_CANARY'
attempt sh -c 'printf "#!/bin/sh\nprintf ESCAPED > HOST_CANARY\n" > .git/hooks/pre-commit'
attempt sh -c 'printf "[core]\nfsmonitor = printf ESCAPED > HOST_CANARY\n" > .git/config.worktree'
attempt sh -c 'printf "../redirect\n" > .git/commondir'
attempt mv .git .git-replaced
set -eu
printf 'NATIVE_GIT_OK\n' > committed.txt
git add -- committed.txt
git commit --no-gpg-sign -m 'test: ordinary sandbox commit'
git branch boundary-branch
"##;
    fs::write(root.join("boundary.sh"), script).unwrap();
}

pub fn prove(f: &Fixture, config: &[u8]) {
    let attempts = fs::read_to_string(f.project().join("attempts.txt")).unwrap();
    assert_eq!(attempts.lines().count(), 6);
    assert!(
        attempts.lines().all(|line| !line.ends_with("=0")),
        "{attempts}"
    );
    assert_eq!(config, fs::read(f.project().join(".git/config")).unwrap());
    assert!(!f.project().join(".git/hooks/host-write").exists());
    assert!(!f.project().join(".git/hooks/pre-commit").exists());
    assert!(!f.project().join("HOST_CANARY").exists());
    git(f.project(), &["show", "HEAD:committed.txt"]);
    let rows = f.rows();
    let parts = rows
        .iter()
        .flat_map(|r| r["message"]["content"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    let write = parts
        .iter()
        .find(|p| {
            p["type"] == "tool_use"
                && p["name"] == "Write"
                && p["input"]["file_path"]
                    .as_str()
                    .is_some_and(|s| s.ends_with(".git/hooks/host-write"))
        })
        .expect("native Write was not attempted");
    assert!(parts.iter().any(|p| p["type"] == "tool_result"
        && p["tool_use_id"] == write["id"]
        && p["is_error"] == true));
}

pub fn fresh_codex(home: &Path, project: &Path) {
    let args = Arguments::parse(vec![
        "--model".into(),
        "gpt-6-sol".into(),
        "--effort".into(),
        "low".into(),
    ])
    .unwrap();
    let launch = Launch::prepare(home, project, args).unwrap();
    let mut command = launch.command(false);
    command.args([
        "--no-alt-screen",
        "Reply READY. Do not use tools or change any files.",
    ]);
    let mut terminal = pty::Terminal::start(
        &pty::command_line(&command),
        &project.join("codex-start.log"),
    );
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(home, project);
    let id = history::session(&mut reader, project, &mut terminal);
    history::completed_turn(&mut reader, &id, None);
    drop(reader);
    drop(terminal);
    assert!(
        Command::new(home.join(".local/bin/codex"))
            .args(["delete", "--force", &id])
            .env_remove("CODEX_THREAD_ID")
            .output()
            .unwrap()
            .status
            .success()
    );
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    if let Some(state) = kah_observer::session::Store::read(&base, &id).unwrap() {
        assert_eq!(state.binding.project, project);
        if let Some(run) = state.run_id {
            let path = kah_core::run_store::RunStore::path_for(&base, project, run).unwrap();
            fs::remove_dir_all(path).unwrap();
        }
    }
    let _ = fs::remove_dir_all(kah_observer::session::Store::directory(&base, &id));
    use sha2::{Digest, Sha256};
    let _ = fs::remove_file(
        base.join("session-policies")
            .join(format!("{:x}.json", Sha256::digest(id.as_bytes()))),
    );
    let _ = fs::remove_dir_all(&launch.overlay);
}
pub fn fresh_claude(f: &Fixture) {
    use kah_observer::session::Store;
    let id = f.id.replacen("00000000", "00000001", 1);
    let command = format!(
        "cd {} && {} --agent claude --model haiku --effort low -- --session-id {} --permission-mode acceptEdits",
        pty::quote(f.project().to_str().unwrap()),
        pty::quote(env!("CARGO_BIN_EXE_kah")),
        pty::quote(&id)
    );
    let mut terminal = pty::Terminal::start(&command, &f.project().join("fresh-claude.log"));
    f.ready(&mut terminal);
    f.wait(&mut terminal, || {
        Store::read(&f.base, &id).ok().flatten().is_some()
    });
    drop(terminal);
    let state = Store::read(&f.base, &id).unwrap().unwrap();
    assert_eq!(state.binding.project, f.project());
    assert!(state.run_id.is_none()); // No model turn or new user intent.
    fs::remove_dir_all(Store::directory(&f.base, &id)).unwrap();
}
pub fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
pub fn version(agent: &str) -> String {
    let path = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".local/bin")
        .join(agent);
    String::from_utf8(Command::new(path).arg("--version").output().unwrap().stdout)
        .unwrap()
        .trim()
        .into()
}
pub fn save(name: &str, value: Value) {
    let root = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(root.join(name), serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}
