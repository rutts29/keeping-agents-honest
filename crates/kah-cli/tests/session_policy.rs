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
