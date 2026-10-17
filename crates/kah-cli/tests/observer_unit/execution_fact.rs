use super::*;
use std::path::PathBuf;

#[test]
fn the_domain_comes_from_policy_or_project_path_and_names_the_sandbox() {
    // Interactive Mine uses the host. Unattended execution is tested separately.
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

fn worker(project: &Path) -> State {
    serde_json::from_value(json!({
        "worker":{"provider":"codex","session":"execution-fact","project":project,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"observer","workspace":"/unused","model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
    })).unwrap()
}

#[test]
fn unattended_commands_use_the_launch_domain_and_keep_the_source_domain() {
    let base = kah_core::run_store::RunStore::default_state_root()
        .unwrap()
        .join("runs");
    for domain in ["mine", "research", "untrusted"] {
        let id = format!("run-{}", crate::relay_store::nonce());
        let root = base.join(&id);
        let project = root.join("worktree");
        let record = kah_run::store::Record {
            project: PathBuf::from(format!("/source/{domain}/project")),
            worktree: project.clone(),
            domain: domain.into(),
            model: None,
            effort: None,
            control: kah_core::unattended::RunControl::new("task", Default::default()).unwrap(),
            native: None,
        };
        drop(kah_run::store::Store::create(&base, &id, record).unwrap());
        let execution = fact(Path::new("/home/u"), &worker(&project));
        std::fs::remove_dir_all(root).unwrap();
        assert_eq!(execution["domain"], domain);
        assert_eq!(execution["commands"], "gVisor sandbox", "{domain}");
    }
}

#[test]
fn interactive_commands_keep_the_existing_domain_mapping() {
    let home = Path::new("/home/u");
    for (domain, expected) in [
        ("mine", "host shell; no gVisor sandbox"),
        ("research", "gVisor sandbox"),
        ("untrusted", "gVisor sandbox"),
    ] {
        let execution = fact(
            home,
            &worker(&home.join("repos").join(domain).join("project")),
        );
        assert_eq!(execution["domain"], domain);
        assert_eq!(execution["commands"], expected);
    }
}
