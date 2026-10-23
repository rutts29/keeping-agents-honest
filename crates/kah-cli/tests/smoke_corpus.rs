//! Closed replay data is copied independently; the helper never reads real HOME.
use std::{fs, os::unix::fs::MetadataExt, process::Command};

#[test]
#[ignore = "host setup: sandbox service account resolved by the smoke environment script"]
fn shared_smoke_setup_copies_the_explicit_closed_corpus_without_shared_inodes() {
    let root = std::env::temp_dir().join(format!("kah-smoke-corpus-{}", std::process::id()));
    for path in ["home", "claude", "seeds/kah-eval/completion"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    fs::write(root.join(".smoke-private"), "fixture owned\n").unwrap();
    let seed = root.join("seeds/kah-eval/completion/retained.json");
    fs::write(&seed, "{\"evidence\":\"whole retained record\"}\n").unwrap();
    let script =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/smoke-environment.sh");
    let output = Command::new("bash")
        .args(["-c", "source \"$1\"", "smoke"])
        .arg(script)
        .env("KAH_SMOKE_ROOT", &root)
        .env("KAH_SMOKE_SEEDS", root.join("seeds"))
        .env("KAH_SMOKE_CONFIG_HOME", root.join("config"))
        .env("HOME", root.join("home"))
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let destination = root.join("home/.local/share/kah-eval/completion/retained.json");
    assert!(
        destination.is_file(),
        "the named replay corpus was not seeded"
    );
    assert_eq!(fs::read(&destination).unwrap(), fs::read(&seed).unwrap());
    assert_ne!(
        fs::metadata(&destination).unwrap().ino(),
        fs::metadata(&seed).unwrap().ino()
    );
    fs::write(&destination, "fixture-only edit\n").unwrap();
    assert_eq!(
        fs::read_to_string(seed).unwrap(),
        "{\"evidence\":\"whole retained record\"}\n"
    );
    assert!(!root.join("home/.codex/auth.json").exists());
    fs::remove_dir_all(root).unwrap();
}
