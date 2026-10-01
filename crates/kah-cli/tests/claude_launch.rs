use kah_cli::{Arguments, claude_launch};
use std::{fs, path::PathBuf};

#[test]
fn mine_preserves_choices_and_installs_only_invocation_hooks() {
    let home = fixture();
    let cwd = home.join("repos/mine/project");
    let args = Arguments::parse(vec!["--agent".into(), "claude".into()]).unwrap();
    let command = claude_launch::prepare(&home, &cwd, args).unwrap();
    let args = command
        .get_args()
        .map(|s| s.to_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(args[0], "--settings");
    assert_eq!(args.len(), 2);
    let settings: serde_json::Value = serde_json::from_str(args[1]).unwrap();
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "Stop",
    ] {
        assert_eq!(settings["hooks"][event][0]["hooks"][0]["type"], "command");
    }
    assert!(!home.join(".claude/settings.json").exists());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn restricted_claude_refuses_native_permission_bypass_before_startup() {
    let home = fixture();
    for domain in ["research", "untrusted"] {
        let cwd = home.join(format!("repos/{domain}/project"));
        fs::create_dir_all(&cwd).unwrap();
        let args = Arguments {
            no_observer: true,
            native: vec!["--dangerously-skip-permissions".into()],
            ..Arguments::default()
        };
        let error = claude_launch::prepare(&home, &cwd, args).unwrap_err();
        assert!(error.to_string().contains("CLAUDE_PERMISSION_POLICY"));
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn inherited_modes_that_disable_hooks_are_reported_before_native_startup() {
    let home = fixture();
    for variable in ["CLAUDE_CODE_SAFE_MODE", "CLAUDE_CODE_SIMPLE"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_kah"))
            .args(["--agent", "claude", "--", "--help"])
            .current_dir(home.join("repos/mine/project"))
            .env("HOME", &home)
            .env_remove("CLAUDE_CODE_SAFE_MODE")
            .env_remove("CLAUDE_CODE_SIMPLE")
            .env(variable, "1")
            .output()
            .unwrap();
        assert!(String::from_utf8_lossy(&output.stderr).contains("CLAUDE_OBSERVER_POLICY"));
    }
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn versioned_native_install_preserves_the_claude_process_name() {
    let home = fixture();
    let version = home.join("versions/2.1.286");
    fs::create_dir(version.parent().unwrap()).unwrap();
    fs::copy("/usr/bin/sleep", &version).unwrap();
    fs::remove_file(home.join(".local/bin/claude")).unwrap();
    std::os::unix::fs::symlink(&version, home.join(".local/bin/claude")).unwrap();
    let args = Arguments {
        no_observer: true,
        native: vec!["10".into()],
        ..Arguments::default()
    };
    let mut child = claude_launch::prepare(&home, &home.join("repos/mine/project"), args)
        .unwrap()
        .spawn()
        .unwrap();
    let name = fs::read_to_string(format!("/proc/{}/comm", child.id()));
    let _ = child.kill();
    child.wait().unwrap();
    fs::remove_dir_all(home).unwrap();
    assert_eq!(name.unwrap().trim(), "claude");
}

fn fixture() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "kah-claude-launch-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(path.join("repos/mine/project/.git")).unwrap();
    fs::create_dir_all(path.join(".local/bin")).unwrap();
    fs::write(path.join(".local/bin/claude"), "fixture").unwrap();
    path
}
