use super::*;
use std::path::PathBuf;

#[test]
fn the_domain_comes_from_policy_or_project_path_and_names_the_sandbox() {
    // A Mine run cannot meet a Research-only rule such as safe.directory in the
    // sandbox; the observer needs this host fact to mark it not applicable.
    assert_eq!(policy_domain("research-online"), Some("research"));
    assert_eq!(policy_domain("mine-yolo"), Some("mine"));
    assert_eq!(policy_domain("other"), None);
    let home = PathBuf::from("/home/u");
    assert_eq!(
        path_domain(&home, &home.join("repos/mine/obs-lab")),
        Some("mine")
    );
    assert_eq!(
        path_domain(&home, &home.join("repos/untrusted/x/y")),
        Some("untrusted")
    );
    assert_eq!(
        path_domain(&home, Path::new("/var/tmp/kah-runs/run-1/worktree")),
        None
    );
    assert_eq!(commands("mine"), "host shell; no gVisor sandbox");
    assert_eq!(commands("research"), "gVisor sandbox");
    assert_eq!(commands("unknown"), "unknown");
}

#[test]
fn every_review_stage_carries_the_execution_fact() {
    let fact = json!({"domain":"mine","commands":commands("mine")});
    for stage in ["checkpoint", "confirmation", "selection"] {
        let job = attach(json!({"stage":stage,"input":{}}), fact.clone());
        assert_eq!(job["execution"], fact, "{stage}");
    }
    let rules = attach(json!({"stage":"current_rules"}), fact.clone());
    assert!(rules.get("execution").is_none());
}
