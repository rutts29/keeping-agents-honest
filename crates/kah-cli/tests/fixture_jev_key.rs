#[path = "fixture_support/jev_key.rs"]
mod jev_key;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn private_config_accepts_policy_writes_and_removes_fixture_key() {
    let root = std::env::temp_dir().join(format!("kah-private-key-test-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let actual = root.join("operator");
    let home = root.join("fixture");
    fs::create_dir_all(actual.join(".config/kah")).unwrap();
    let original = actual.join(".config/kah/typesafe.key");
    fs::write(&original, b"fixture-only-dummy").unwrap();
    jev_key::install(&actual, &home);
    let dir = home.join(".config/kah");
    assert!(fs::symlink_metadata(&dir).unwrap().file_type().is_dir());
    assert_eq!(
        fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
    kah_core::run_store::ensure_private_directory(&dir).unwrap();
    fs::write(dir.join("commit-policy.json"), b"{}").unwrap();
    let audit = jev_key::cleanup(&home);
    assert_eq!(audit["passed"], true);
    assert_eq!(audit["removed"], true);
    assert!(fs::symlink_metadata(dir.join("typesafe.key")).is_err());
    assert!(dir.join("commit-policy.json").is_file());
    assert_eq!(fs::read(original).unwrap(), b"fixture-only-dummy");
    fs::remove_dir_all(root).unwrap();
}
