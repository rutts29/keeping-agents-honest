use kah_cli::session_policy::{SessionPolicy, validate};
use serde_json::json;
use std::fs;

#[test]
fn same_policy_resumes_but_cross_domain_project_and_yolo_fail_closed() {
    let base = std::env::temp_dir().join(format!("kah-policy-{}", std::process::id()));
    fs::create_dir_all(base.join("project")).unwrap();
    let policy = SessionPolicy {
        project: base.join("project"),
        policy: "research-offline".into(),
        overlay: base.join("overlay"),
    };
    fs::create_dir_all(&policy.overlay).unwrap();
    fs::write(policy.overlay.join("environments.toml"), "default='kah-research-offline'\ninclude_local=false\n[[environments]]\nid='kah-research-offline'\nprogram='/usr/bin/docker'\nargs=['run','--runtime','runsc']\n").unwrap();
    let mut input = json!({"session_id":"fixture","cwd":policy.project,"hook_event_name":"SessionStart","source":"startup"});
    validate(&base, &policy, &input).unwrap();
    input["source"] = json!("resume");
    validate(&base, &policy, &input).unwrap();
    for name in [
        "mine-offline",
        "mine-yolo",
        "research-online",
        "untrusted-offline",
    ] {
        let mut other = policy.clone();
        other.policy = name.into();
        assert!(
            validate(&base, &other, &input)
                .unwrap_err()
                .contains("reopen with kah research project")
        );
    }
    let mut other = policy.clone();
    other.project = base.join("other");
    assert!(validate(&base, &other, &input).is_err());
    input["session_id"] = json!("unknown");
    assert!(
        validate(&base, &policy, &input)
            .unwrap_err()
            .starts_with("SESSION_POLICY_UNKNOWN")
    );
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn restricted_resume_and_pre_tool_use_refuse_a_missing_or_local_executor() {
    let base = std::env::temp_dir().join(format!("kah-policy-executor-{}", std::process::id()));
    fs::create_dir_all(base.join("project")).unwrap();
    fs::create_dir_all(base.join("overlay")).unwrap();
    let policy = SessionPolicy {
        project: base.join("project"),
        policy: "research-offline".into(),
        overlay: base.join("overlay"),
    };
    let input = json!({"session_id":"executor-fixture","cwd":policy.project,"hook_event_name":"SessionStart","source":"startup"});
    let manifest = policy.overlay.join("environments.toml");
    fs::write(&manifest, "default='kah-research-offline'\ninclude_local=false\n[[environments]]\nid='kah-research-offline'\nprogram='/usr/bin/docker'\nargs=['run','--runtime','runsc']\n").unwrap();
    validate(&base, &policy, &input).unwrap();
    fs::remove_file(&manifest).unwrap();
    for event in ["SessionStart", "UserPromptSubmit", "PreToolUse"] {
        let mut resumed = input.clone();
        resumed["hook_event_name"] = event.into();
        resumed["source"] = "resume".into();
        assert!(
            validate(&base, &policy, &resumed)
                .unwrap_err()
                .starts_with("SESSION_EXECUTOR_POLICY")
        );
    }
    for content in [
        "default='local'\ninclude_local=true\n",
        "default='kah-research-offline'\ninclude_local=true\n[[environments]]\nid='kah-research-offline'\nprogram='/usr/bin/docker'\nargs=['run','--runtime','runsc']\n",
        "default='kah-untrusted-online'\ninclude_local=false\n[[environments]]\nid='kah-untrusted-online'\nprogram='/usr/bin/docker'\nargs=['run','--runtime','runsc']\n",
        "default='kah-research-offline'\ninclude_local=false\nenvironments=[]\n",
    ] {
        fs::write(&manifest, content).unwrap();
        assert!(
            validate(&base, &policy, &input)
                .unwrap_err()
                .starts_with("SESSION_EXECUTOR_POLICY")
        );
    }
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn pre_tool_rejection_uses_native_deny_instead_of_unsupported_universal_fields() {
    let reason = "SESSION_EXECUTOR_POLICY stage=hook: missing executor";
    let output =
        kah_cli::session_policy::rejection(&json!({"hook_event_name":"PreToolUse"}), reason);
    assert_eq!(output["hookSpecificOutput"]["permissionDecision"], "deny");
    assert_eq!(
        output["hookSpecificOutput"]["permissionDecisionReason"],
        reason
    );
    assert!(output.get("continue").is_none());
    assert!(output.get("stopReason").is_none());
    let startup =
        kah_cli::session_policy::rejection(&json!({"hook_event_name":"SessionStart"}), reason);
    assert_eq!(startup["continue"], false);
}

#[test]
fn hook_process_blocks_missing_executor_before_observer_work() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let root = std::env::temp_dir().join(format!("kah-hook-executor-{}", std::process::id()));
    fs::create_dir_all(root.join("project")).unwrap();
    let policy = SessionPolicy {
        project: root.join("project"),
        policy: "research-offline".into(),
        overlay: root.join("missing-overlay"),
    };
    fs::create_dir_all(&policy.overlay).unwrap();
    let manifest = policy.overlay.join("environments.toml");
    fs::write(&manifest, "default='kah-research-offline'\ninclude_local=false\n[[environments]]\nid='kah-research-offline'\nprogram='/usr/bin/docker'\nargs=['run','--runtime','runsc']\n").unwrap();
    validate(&root.join("state/kah"), &policy, &json!({"session_id":"missing-executor-session","hook_event_name":"SessionStart","source":"startup","cwd":policy.project})).unwrap();
    fs::remove_file(manifest).unwrap();
    for event in ["SessionStart", "UserPromptSubmit", "PreToolUse"] {
        let input = json!({"session_id":"missing-executor-session","hook_event_name":event,"source":"resume","cwd":policy.project});
        let mut child = Command::new(env!("CARGO_BIN_EXE_kah"))
            .arg("__hook")
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_STATE_HOME", root.join("state"))
            .env(
                "KAH_SESSION_POLICY",
                serde_json::to_string(&policy).unwrap(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&input).unwrap())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let rejection: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(
            rejection["systemMessage"]
                .as_str()
                .unwrap()
                .starts_with("SESSION_EXECUTOR_POLICY")
        );
        if event == "PreToolUse" {
            assert_eq!(
                rejection["hookSpecificOutput"]["permissionDecision"],
                "deny"
            );
        } else {
            assert_eq!(rejection["continue"], false);
        }
        assert!(!root.join("state/kah/observer-sessions").exists());
    }
    fs::remove_dir_all(root).unwrap();
}
