#[allow(dead_code)]
#[path = "track2_support/claude_login.rs"]
mod login;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::Command,
};
fn root() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "kah-login-test-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    fs::create_dir_all(path.join(".claude")).unwrap();
    for p in [&path, &path.join(".claude")] {
        fs::set_permissions(p, fs::Permissions::from_mode(0o700)).unwrap();
    }
    path
}
#[test]
fn metadata_only_login_gate_requires_user_success_marker_and_private_files() {
    let path = root();
    assert!(login::ready_at(&path).is_err());
    for p in [
        path.join(".claude/.credentials.json"),
        path.join("test-login-ready"),
    ] {
        fs::File::create(&p).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert_eq!(login::ready_at(&path), Ok(()));
    fs::set_permissions(path.join(".claude"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(login::ready_at(&path).is_err());
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn atomic_credential_replacement_fails_without_opening_contents() {
    let path = root();
    let target = path.join("template-login");
    let link = path.join(".claude/.credentials.json");
    symlink(&target, &link).unwrap();
    assert_eq!(login::audit_link(&link, &target)["symlink_unchanged"], true);
    fs::remove_file(&link).unwrap();
    fs::File::create(&link).unwrap();
    let result = login::audit_link(&link, &target);
    assert_eq!(result["symlink_unchanged"], false);
    assert_eq!(
        result["reason"],
        "TRACK2_LOGIN_LINK_CHANGED stage=login_audit"
    );
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn test_environment_disables_updates_and_removes_alternate_auth_inputs() {
    let mut command = Command::new("unused");
    for key in login::AUTH_ENV {
        command.env(key, "unused-test-sentinel");
    }
    login::command_environment(&mut command);
    for key in login::AUTH_ENV {
        assert!(command.get_envs().any(|(k, v)| k == *key && v.is_none()));
    }
    assert!(
        command
            .get_envs()
            .any(|(k, v)| k == "DISABLE_AUTOUPDATER" && v == Some(std::ffi::OsStr::new("1")))
    );
    assert!(
        !command
            .get_envs()
            .any(|(k, _)| k.to_string_lossy().to_ascii_lowercase().contains("fast"))
    );
}
