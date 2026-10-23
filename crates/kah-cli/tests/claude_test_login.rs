#[allow(dead_code)]
#[path = "track2_support/claude_login.rs"]
mod login;
#[path = "track2_support/claude_metadata.rs"]
mod metadata;
mod claude_login {
    pub use super::login::CONFIG_DIR;
}
#[path = "track2_support/claude_invocation.rs"]
mod invocation;
#[path = "track2_support/claude_trust.rs"]
mod trust;
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
    if let Ok(uid) = kah_run::access::service_uid() {
        for directory in [&path, &path.join(".claude")] {
            kah_run::access::traverse(directory, uid).unwrap();
        }
        assert_eq!(login::ready_at(&path), Ok(()));
        assert!(
            Command::new("/usr/bin/setfacl")
                .args(["-m", &format!("u:{uid}:r-x")])
                .arg(path.join(".claude"))
                .status()
                .unwrap()
                .success()
        );
        assert!(login::ready_at(&path).is_err());
    }
    fs::set_permissions(path.join(".claude"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(login::ready_at(&path).is_err());
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn any_fixture_credential_file_or_link_fails_without_opening_contents() {
    let path = root();
    let operator = path.join("operator");
    fs::create_dir(&operator).unwrap();
    fs::File::create(operator.join(".credentials.json")).unwrap();
    let home = path.join("fixture");
    fs::create_dir_all(home.join("nested/.claude")).unwrap();
    let before = metadata::snapshot(&operator);
    assert_eq!(
        metadata::audit(std::slice::from_ref(&home), &operator, &before, &[], &[])["passed"],
        true
    );
    let credential = home.join("nested/.claude/.credentials.json");
    symlink(path.join("nonexistent"), &credential).unwrap();
    assert_eq!(
        metadata::audit(std::slice::from_ref(&home), &operator, &before, &[], &[])["passed"],
        false
    );
    fs::remove_file(&credential).unwrap();
    fs::File::create(&credential).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o0)).unwrap();
    assert_eq!(
        metadata::audit(&[home], &operator, &before, &[], &[])["fixture_claude_credentials_absent"],
        false
    );
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn unrelated_live_metadata_changes_are_informational_and_credential_contents_stay_closed() {
    let path = root();
    let operator = path.join("operator");
    fs::create_dir(&operator).unwrap();
    fs::File::create(operator.join(".credentials.json")).unwrap();
    symlink(path.join("never-read"), operator.join("outside")).unwrap();
    let before = metadata::snapshot(&operator);
    assert!(before["errors"].as_array().unwrap().is_empty());
    fs::File::create(operator.join("unrelated-live-session")).unwrap();
    let result = metadata::audit(&[], &operator, &before, &[], &[]);
    assert_eq!(result["other_metadata_changes_informational"], true);
    assert_eq!(result["operator_credential_regular"], true);
    assert_eq!(result["credential_contents_opened"], false);
    assert_eq!(result["passed"], true);
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn per_invocation_settings_preserve_hooks_and_argument_boundaries() {
    let path = root();
    fs::create_dir_all(path.join(".local/bin")).unwrap();
    let native = path.join("fixture-native");
    fs::write(
        &native,
        "#!/bin/bash\nprintf '%s\\n' \"$CLAUDE_CONFIG_DIR\" \"$@\"\n",
    )
    .unwrap();
    fs::set_permissions(&native, fs::Permissions::from_mode(0o700)).unwrap();
    invocation::install(&path, &native);
    let output = invocation::output_retrying_text_file_busy(|| {
        let mut command = Command::new(path.join(".local/bin/claude"));
        command.args([
            "--settings",
            r#"{"hooks":{"Stop":[{"fixture":true}]}}"#,
            "--",
            "prompt with 'quotes' $literal",
        ]);
        command
    });
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    let lines = text.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], login::CONFIG_DIR);
    let settings: serde_json::Value = serde_json::from_str(lines[2]).unwrap();
    assert_eq!(settings["hooks"]["Stop"][0]["fixture"], true);
    assert_eq!(settings["fastMode"], false);
    assert_eq!(settings["model"], "claude-sonnet-5-5");
    assert_eq!(lines[4], "prompt with 'quotes' $literal");
    fs::remove_dir_all(path).unwrap();
}
#[test]
fn test_environment_disables_updates_and_removes_alternate_auth_inputs() {
    let mut command = Command::new("unused");
    for key in login::AUTH_ENV {
        command.env(key, "unused-test-sentinel");
    }
    login::command_environment(&mut command);
    assert!(command.get_envs().any(|(k, v)| k == "CLAUDE_CONFIG_DIR"
        && v == Some(std::ffi::OsStr::new(
            "/var/tmp/kah-claude-test-home/.claude"
        ))));
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

#[test]
fn trust_receipts_remove_added_entries_restore_prior_flags_and_reject_home_root() {
    let path = root();
    let project = path.join("private-project");
    fs::create_dir(&project).unwrap();
    let existing = path.join("existing-project");
    fs::create_dir(&existing).unwrap();
    let config = path.join("test-global.json");
    fs::write(&config,serde_json::json!({"unrelated_counter":10,"projects":{existing.to_str().unwrap():{"hasTrustDialogAccepted":false,"counter":3}}}).to_string()).unwrap();
    let receipts = path.join("trust-receipts.json");
    assert!(trust::add(&config, &path, &path, &receipts).is_err());
    trust::add(&config, &path, &project, &receipts).unwrap();
    trust::add(&config, &path, &existing, &receipts).unwrap();
    trust::add(&config, &path, &project, &receipts).unwrap();
    let result = trust::cleanup(&config, &receipts).unwrap();
    assert_eq!(result["passed"], true);
    let after: serde_json::Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert!(after["projects"].get(project.to_str().unwrap()).is_none());
    assert_eq!(
        after["projects"][existing.to_str().unwrap()]["hasTrustDialogAccepted"],
        false
    );
    assert_eq!(after["projects"][existing.to_str().unwrap()]["counter"], 3);
    assert_eq!(after["unrelated_counter"], 10);
    fs::remove_dir_all(path).unwrap();
}

#[test]
fn attributed_operator_project_and_session_names_fail_without_credential_reads() {
    let path = root();
    let operator = path.join("operator");
    fs::create_dir(&operator).unwrap();
    fs::File::create(operator.join(".credentials.json")).unwrap();
    fs::set_permissions(
        operator.join(".credentials.json"),
        fs::Permissions::from_mode(0o0),
    )
    .unwrap();
    let before = metadata::snapshot(&operator);
    let project = path.join("worktree");
    let encoded = kah_claude::paths::project_key(&project);
    fs::create_dir_all(operator.join("projects").join(encoded)).unwrap();
    assert_eq!(
        metadata::audit(&[], &operator, &before, &[], &[project])["passed"],
        false
    );
    fs::File::create(operator.join("fixture-session.jsonl")).unwrap();
    let result = metadata::audit(&[], &operator, &before, &["fixture-session".into()], &[]);
    assert_eq!(result["passed"], false);
    assert_eq!(result["credential_contents_opened"], false);
    fs::remove_dir_all(path).unwrap();
}
