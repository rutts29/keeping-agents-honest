//! Candidate traversal must not change the private session-store ACL.
use super::*;
// One shared builder; each test module includes its own copy.
#[allow(clippy::duplicate_mod)]
#[path = "synthetic_gate.rs"]
mod synthetic;
use crate::observer_review::state::{State, Store};
use kah_observer::session::Store as Sessions;
use std::{os::unix::fs::PermissionsExt, time::Duration};

fn acl(path: &Path) -> String {
    let output = Command::new("getfacl")
        .args(["-cpn"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn gate_candidate_traversal_preserves_strict_session_privacy() {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let worker = format!("vf16-privacy-{}", crate::relay_store::nonce());
    drop(Sessions::open(&base, &worker, Duration::ZERO).unwrap());
    let session_root = Sessions::directory(&base, &worker);
    let before = acl(&session_root);
    let root = Store::directory(&worker)
        .unwrap()
        .join("interactive-gates/review-vf16");
    kah_core::run_store::ensure_save_directory(&root).unwrap();
    let home = std::env::temp_dir().join(format!("vf16-home-{}", crate::relay_store::nonce()));
    kah_core::run_store::ensure_save_directory(&home.join("source")).unwrap();
    let (candidate, head) = tagged_candidate(&root, &home, &worker);
    let snapshot = candidate
        .verification_snapshot(&candidate.path.parent().unwrap().join("verify-vf16"), &head)
        .unwrap();
    let audit = crate::exact_verification::prepare_access(
        &snapshot.path,
        kah_run::access::service_uid().unwrap(),
    )
    .unwrap();
    let reopened = Sessions::open(&base, &worker, Duration::ZERO);
    assert!(
        reopened.is_ok(),
        "VF16 after traversal: {:?}",
        reopened.err()
    );
    assert_eq!(acl(&session_root), before);
    assert_eq!(
        fs::metadata(&session_root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert!(
        audit
            .as_array()
            .unwrap()
            .iter()
            .all(|r| !Path::new(r["path"].as_str().unwrap()).starts_with(&base))
    );
    println!(
        "VF16_SESSION_PRIVATE_PASS before={before} after={} audit={audit}",
        acl(&session_root)
    );
    snapshot.remove_verification().unwrap();
    fs::remove_dir_all(session_root).unwrap();
    if home.exists() {
        fs::remove_dir_all(home).unwrap();
    }
}

fn tagged_candidate(root: &Path, home: &Path, worker: &str) -> (Worktree, String) {
    let state: State = serde_json::from_value(json!({
        "worker":{"provider":"claude","session":worker,"project":home.join("source"),"pane":null,"socket":null},
        "observer":{"session":"recorded","pane":null,"name":"recorded","workspace":home.join(".cache/kah-observers/fixture"),
          "model":"claude-sonnet-5-5","effort":"low","transcript":null},
        "pending":null,"latest":null,"checkpoint":null,"last_verified_turn":null,
        "deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    let head = synthetic::tagged_bundle(&home.join("origin"), &root.join("source.bundle"));
    (load(root, home, &state, &head).unwrap(), head)
}

#[test]
#[ignore = "host setup: sandbox service account and host ACL sharing"]
fn private_state_candidate_is_rejected_before_any_acl_changes() {
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    let worker = format!("vf16-forbidden-{}", crate::relay_store::nonce());
    drop(Sessions::open(&base, &worker, Duration::ZERO).unwrap());
    let session = Sessions::directory(&base, &worker);
    let bind = session.join("review/interactive-gates/retained/candidate/verify");
    kah_core::run_store::ensure_save_directory(&bind).unwrap();
    let before = acl(&session);
    let error =
        crate::exact_verification::prepare_access(&bind, kah_run::access::service_uid().unwrap())
            .unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("RUN_VERIFIER_INFRA_GAP stage=verification")
    );
    assert_eq!(acl(&session), before);
    drop(Sessions::open(&base, &worker, Duration::ZERO).unwrap());
    fs::remove_dir_all(session).unwrap();
}
