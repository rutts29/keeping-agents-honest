use super::*;
use std::os::unix::fs::symlink;

#[test]
fn config_audit_retains_nested_policy_modes_digests_and_link_targets() {
    let root = std::env::temp_dir().join(format!(
        "kah-config-audit-{}",
        kah_cli::relay_store::nonce()
    ));
    fs::create_dir_all(root.join("commit-policy/history")).unwrap();
    fs::write(root.join("typesafe.key"), "fake test key").unwrap();
    let file = root.join("commit-policy/history/line.json");
    fs::write(&file, "{\"line\":0.5}").unwrap();
    symlink("history/line.json", root.join("commit-policy/current")).unwrap();
    let before = config_snapshot(&root);
    assert_eq!(before, config_snapshot(&root));
    assert!(
        before
            .to_string()
            .contains("commit-policy/history/line.json")
    );
    assert!(!before.to_string().contains("fake test key"));
    fs::write(&file, "{\"line\":0.6}").unwrap();
    assert_ne!(before, config_snapshot(&root));
    fs::write(&file, "{\"line\":0.5}").unwrap();
    assert_eq!(before, config_snapshot(&root));
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    assert_ne!(before, config_snapshot(&root));
    let modes = config_snapshot(&root);
    fs::write(root.join("commit-policy/new.json"), "{}").unwrap();
    assert_ne!(modes, config_snapshot(&root));
    fs::remove_file(root.join("commit-policy/new.json")).unwrap();
    fs::remove_file(root.join("commit-policy/current")).unwrap();
    symlink("different-target", root.join("commit-policy/current")).unwrap();
    assert_ne!(modes, config_snapshot(&root));
    fs::remove_dir_all(root).unwrap();
}
