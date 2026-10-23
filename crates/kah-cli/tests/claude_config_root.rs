//! Native configuration-root inheritance and repository-setting isolation.
//! Invocations inherit the operator's directory; repo settings cannot set it.
use serde_json::json;
use std::{fs, path::PathBuf, process::Command};
#[test]
fn configured_native_root_is_validated_and_credentials_stay_protected() {
    let root = std::env::temp_dir().join(format!("kah-config-root-{}", std::process::id()));
    let home = root.join("home");
    let config = root.join("native");
    fs::create_dir_all(home.join("repos/research/current")).unwrap();
    fs::create_dir_all(config.join("skills")).unwrap();
    fs::create_dir_all(config.join("projects/native-key")).unwrap();
    fs::File::create(config.join(".credentials.json")).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "config_child", "--nocapture"])
        .env("KAH_CONFIG_ROOT_TEST", &root)
        .env("CLAUDE_CONFIG_DIR", &config)
        .output()
        .unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn config_child() {
    let Some(root) = std::env::var_os("KAH_CONFIG_ROOT_TEST").map(PathBuf::from) else {
        return;
    };
    let home = root.join("home");
    let config = root.join("native");
    let session = "native-session";
    let transcript = config
        .join("projects/native-key")
        .join(format!("{session}.jsonl"));
    assert!(kah_cli::claude_transcript::validate(&home, session, &transcript).is_ok());
    assert!(
        kah_cli::claude_transcript::validate(
            &home,
            session,
            &home
                .join(".claude/projects/native-key")
                .join(format!("{session}.jsonl"))
        )
        .is_err()
    );
    let outside = root.join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::File::create(outside.join(format!("{session}.jsonl"))).unwrap();
    std::os::unix::fs::symlink(&outside, config.join("projects/escape")).unwrap();
    assert!(
        kah_cli::claude_transcript::validate(
            &home,
            session,
            &config
                .join("projects/escape")
                .join(format!("{session}.jsonl"))
        )
        .is_err()
    );
    let policy = kah_cli::claude_policy::permissions(
        &home,
        &home.join("repos/research/current"),
        &root.join("runtime"),
        false,
        None,
    )
    .unwrap();
    assert!(policy["deny"].as_array().unwrap().contains(&json!(format!(
        "Read(/{})",
        config.join(".credentials.json").display()
    ))));
    assert!(!policy["deny"].as_array().unwrap().contains(&json!(format!(
        "Read(/{}/**)",
        config.join("skills").display()
    ))));
    let mounts = kah_sandbox::global_setup_mounts_with_claude(&home, &config).unwrap();
    assert!(
        mounts
            .iter()
            .any(|(_, target)| target == &config.join("skills"))
    );
    assert!(
        !mounts
            .iter()
            .any(|(_, target)| target == &config.join(".credentials.json"))
    );
    assert!(!home.join(".claude/.credentials.json").exists());
}
