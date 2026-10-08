//! Installed Claude2.1.292 we()/Gbr (203565295/203687833): native config-root
//! NFC normalization, and global .claude.json inside an explicit config dir.
use kah_claude::paths;
use std::{path::Path, process::Command};
#[test]
fn native_config_directory_is_shared_directly_with_no_private_credential_store() {
    for value in [None, Some("/template/.claude"), Some("/template/e\u{301}")] {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "native_config_child", "--nocapture"])
            .env("KAH_NATIVE_CONFIG_CHILD", "1")
            .env_remove("CLAUDE_CONFIG_DIR");
        if let Some(value) = value {
            command.env("CLAUDE_CONFIG_DIR", value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
#[test]
fn native_config_child() {
    if std::env::var_os("KAH_NATIVE_CONFIG_CHILD").is_none() {
        return;
    }
    let home = Path::new("/fixture/home");
    let configured = std::env::var("CLAUDE_CONFIG_DIR").ok();
    let root = match configured.as_deref() {
        None => "/fixture/home/.claude",
        Some("/template/.claude") => "/template/.claude",
        Some(_) => "/template/é",
    };
    assert_eq!(paths::config_root(home), Path::new(root));
    assert_eq!(
        paths::global_config(home),
        configured
            .as_deref()
            .map(Path::new)
            .unwrap_or(home)
            .join(".claude.json")
    );
    assert!(
        paths::transcript(home, Path::new("/project"), "session")
            .starts_with(Path::new(root).join("projects"))
    );
}
