//! Regression for host-side object-only export.
use super::*;
use crate::observer_review::state::State;
use kah_sandbox::{Domain, Project};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;

fn run_git(root: &Path, args: &[&str]) -> String {
    let output = git()
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn inventory(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            result.extend(inventory(&path));
        } else {
            let digest = format!("{:x}", Sha256::digest(fs::read(&path).unwrap()));
            result.insert(path, digest);
        }
    }
    result
}
fn fixture(root: &Path) -> (State, String) {
    let source = root.join("source");
    fs::create_dir_all(&source).unwrap();
    run_git(&source, &["init", "--template="]);
    fs::write(
        source.join("check.cjs"),
        "require('node:assert/strict').equal(6*7,42);\n",
    )
    .unwrap();
    run_git(&source, &["add", "."]);
    run_git(&source, &["commit", "-m", "fixture"]);
    let linked = root.join("linked");
    run_git(&source, &["worktree", "add", linked.to_str().unwrap()]);
    let head = run_git(&linked, &["rev-parse", "HEAD"]);
    let state: State = serde_json::from_value(json!({
        "worker":{"provider":"claude","session":"retained-linked-gate","project":linked,"pane":null,"socket":null},
        "observer":{"session":"recorded-observer","pane":null,"name":"recorded-observer","workspace":root,
          "model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":null,"checkpoint":null,"last_verified_turn":null,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    (state, head)
}

#[test]
fn linked_worker_exports_only_objects_and_never_carries_or_executes_source_control() {
    let root = std::env::temp_dir().join(format!("linked-gate-{}", crate::relay_store::nonce()));
    fs::create_dir_all(&root).unwrap();
    let (state, head) = fixture(&root);
    let admin = root.join("source/.git");
    fs::create_dir(admin.join("hooks")).unwrap();
    fs::write(
        admin.join("hooks/post-checkout"),
        format!(
            "#!/bin/sh\ntouch '{}'\n",
            root.join("SOURCE_HOOK_RAN").display()
        ),
    )
    .unwrap();
    fs::set_permissions(
        admin.join("hooks/post-checkout"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    run_git(
        &state.worker.project,
        &["config", "fixture.secret", "ADMIN_CONFIG_MUST_NOT_TRANSFER"],
    );
    let before = inventory(&admin);
    let candidate_root = root.join("gate");
    fs::create_dir(&candidate_root).unwrap();
    let candidate = load(&candidate_root, &state, &head).unwrap();
    assert_eq!(candidate.head().unwrap(), head);
    verify_candidate(&root, &candidate);
    assert_eq!(before, inventory(&admin), "host export must be read-only");
    sandbox_still_refuses(&state);
    fs::remove_dir_all(root).unwrap();
}
fn verify_candidate(root: &Path, candidate: &Worktree) {
    assert!(
        Command::new("node")
            .arg("check.cjs")
            .current_dir(&candidate.path)
            .status()
            .unwrap()
            .success()
    );
    assert!(!candidate.git_dir.join("hooks/post-checkout").exists());
    assert!(
        !fs::read_to_string(candidate.git_dir.join("config"))
            .unwrap()
            .contains("ADMIN_CONFIG_MUST_NOT_TRANSFER")
    );
    assert!(!root.join("SOURCE_HOOK_RAN").exists());
}
fn sandbox_still_refuses(state: &State) {
    assert!(
        kah_sandbox::git_guard::protect(
            kah_sandbox::Executor::new(
                &Project {
                    domain: Domain::Research,
                    root: state.worker.project.clone(),
                    relative_cwd: Default::default(),
                    needs_creation: false
                },
                Default::default(),
                Path::new("/usr/bin/true"),
                "fixture",
                kah_run::access::service_uid().unwrap()
            )
            .unwrap(),
            &state.worker.project,
            None
        )
        .is_err(),
        "sandbox boundary stays closed for external admin dirs"
    );
}
