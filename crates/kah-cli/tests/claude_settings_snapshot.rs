#[path = "claude_support/settings.rs"]
mod settings;
use std::{fs, process::Command};

#[test]
fn fixture_settings_use_the_native_config_root_and_detect_changes() {
    if std::env::var_os("KAH_SETTINGS_SNAPSHOT_CHILD").is_some() {
        child();
        return;
    }
    let root = std::env::temp_dir().join(format!("claude-settings-unit-{}", std::process::id()));
    fs::create_dir_all(root.join("home/.claude")).unwrap();
    fs::create_dir_all(root.join("native")).unwrap();
    fs::write(root.join("home/.claude/settings.json"), "wrong root").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fixture_settings_use_the_native_config_root_and_detect_changes",
        ])
        .env("KAH_SETTINGS_SNAPSHOT_CHILD", "1")
        .env("HOME", root.join("home"))
        .env("CLAUDE_CONFIG_DIR", root.join("native"))
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{report}");
    // The child expects one panic (a directory where the file should be). Its
    // text must not reach the suite output, where it reads like a failure.
    assert!(!report.contains("panicked at"), "{report}");
}

fn child() {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    let path = kah_claude::paths::config_root(&home).join("settings.json");
    assert_eq!(settings::snapshot(&home), None);
    let bytes = b"{\"public\":\"whole unchanged settings\"}\n";
    fs::write(&path, bytes).unwrap();
    let before = settings::snapshot(&home);
    assert_eq!(before.as_deref(), Some(bytes.as_slice()));
    assert_eq!(settings::snapshot(&home), before);
    fs::write(&path, b"changed settings\n").unwrap();
    assert_ne!(settings::snapshot(&home), before);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(std::panic::catch_unwind(|| settings::snapshot(&home)).is_err());
}
