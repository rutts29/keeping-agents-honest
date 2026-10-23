//! Smoke setup only: no agents, credentials, provider calls or Docker containers.
use std::{fs, process::Command};
#[test]
#[ignore = "host setup: sandbox service account resolved by the smoke environment script"]
fn shared_smoke_setup_seeds_only_the_marked_private_home() {
    let root = std::env::temp_dir().join(format!("kah-smoke-env-unit-{}", std::process::id()));
    fs::create_dir_all(root.join("home")).unwrap();
    fs::create_dir_all(root.join("seeds/skills/sample")).unwrap();
    fs::create_dir_all(root.join("claude")).unwrap();
    fs::write(root.join(".smoke-private"), "fixture owned\n").unwrap();
    fs::write(root.join("seeds/skills/sample/SKILL.md"), "public skill\n").unwrap();
    std::os::unix::fs::symlink(
        root.join("seeds/skills/sample"),
        root.join("seeds/skills/linked"),
    )
    .unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/smoke-environment.sh");
    let output = Command::new("bash")
        .args([
            "-c",
            "source \"$1\"; printf '%s\\n' \"$XDG_CONFIG_HOME\"",
            "smoke",
        ])
        .arg(script)
        .env("KAH_SMOKE_ROOT", &root)
        .env("KAH_SMOKE_SEEDS", root.join("seeds"))
        .env("KAH_SMOKE_CONFIG_HOME", root.join("config"))
        .env("HOME", root.join("home"))
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for domain in ["mine", "research", "untrusted"] {
        assert!(root.join("home/repos").join(domain).is_dir());
    }
    assert_eq!(
        fs::read(root.join("home/.agents/skills/sample/SKILL.md")).unwrap(),
        b"public skill\n"
    );
    assert!(
        !fs::symlink_metadata(root.join("home/.agents/skills/linked"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        root.join("config").to_str().unwrap()
    );
    assert!(!root.join("home/.claude/.credentials.json").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn shared_smoke_setup_rejects_an_unmarked_home_before_writing() {
    let root = std::env::temp_dir().join(format!("kah-smoke-unmarked-unit-{}", std::process::id()));
    fs::create_dir_all(root.join("home")).unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/smoke-environment.sh");
    let output = Command::new("bash")
        .args(["-c", "source \"$1\"", "smoke"])
        .arg(script)
        .env("KAH_SMOKE_ROOT", &root)
        .env("KAH_SMOKE_SEEDS", root.join("seeds"))
        .env("HOME", root.join("home"))
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("controller marker absent"));
    assert!(!root.join("home/repos").exists());
    fs::remove_dir_all(root).unwrap();
}
