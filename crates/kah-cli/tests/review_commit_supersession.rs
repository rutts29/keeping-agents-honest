//! Official git/git v2.53.0 Documentation/git-merge-base.adoc: --is-ancestor.
use kah_cli::observer_review::{
    queue::{self, Wake, supersede},
    state::{Purpose, State, Store},
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
#[test]
fn enqueue_publishes_new_ancestor_coverage_and_retires_only_the_old_commit() {
    let root = repository("enqueue-ancestry");
    let a = commit(&root, "base");
    let b = commit(&root, "descendant");
    let worker = format!("commit-enqueue-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "gate",
        wake(&a, "gate").trigger,
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "old",
        wake(&a, "commit").trigger,
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "new",
        wake(&b, "commit").trigger,
    )
    .unwrap();
    let jobs = queue::entries(&worker).unwrap();
    assert_eq!(jobs.len(), 2);
    assert!(
        jobs.iter()
            .any(|(_, w)| w.trigger["review_class"] == "gate")
    );
    let latest = &jobs
        .iter()
        .find(|(_, w)| w.trigger["review_class"] == "commit")
        .unwrap()
        .1;
    assert_eq!(latest.trigger["head"], b);
    assert_eq!(
        latest.trigger["user_authority_changes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        fs::read_dir(store.directory.join("queue-retired"))
            .unwrap()
            .count(),
        1
    );
    let state_root = store.directory.parent().unwrap().to_path_buf();
    drop(store);
    fs::remove_dir_all(state_root).unwrap();
    fs::remove_dir_all(root).unwrap();
}
fn commit(root: &Path, text: &str) -> String {
    fs::write(root.join("file"), text).unwrap();
    git(root, &["add", "file"]);
    git(
        root,
        &[
            "-c",
            "user.name=kah fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            text,
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}
fn wake(head: &str, class: &str) -> Wake {
    Wake {
        purpose: Purpose::Checkpoint,
        turn: head.into(),
        trigger: json!({"review_class":class,"head":head,"events":[{"kind":"claim_made","source":"whole native claim"}],"user_authority_change":{"verbatim":"Whole user correction.","receipt":format!("authority-{head}")}}),
    }
}
fn repository(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kah-{name}-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    root
}
#[test]
fn ancestor_transfer_preserves_claim_and_authority_and_keeps_divergence_gate_and_active() {
    let root = std::env::temp_dir().join(format!("kah-ancestry-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    let a = commit(&root, "base");
    let b = commit(&root, "descendant");
    git(&root, &["checkout", "--detach", &a]);
    let c = commit(&root, "divergence");
    let jobs = vec![
        (PathBuf::from("ancestor"), wake(&a, "commit")),
        (PathBuf::from("divergent"), wake(&c, "commit")),
        (PathBuf::from("gate"), wake(&a, "gate")),
    ];
    let mut latest = wake(&b, "commit");
    let removed = supersede::merge(&root, &mut latest, &jobs, None).unwrap();
    assert_eq!(removed, vec![PathBuf::from("ancestor")]);
    assert_eq!(latest.trigger["events"].as_array().unwrap().len(), 2);
    assert_eq!(
        latest.trigger["user_authority_changes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        latest.trigger["superseded_triggers"][0]["user_authority_change"],
        jobs[0].1.trigger["user_authority_change"]
    );
    assert_eq!(
        latest.trigger["superseded_triggers"][0]["events"],
        jobs[0].1.trigger["events"]
    );
    let mut active = wake(&b, "commit");
    assert!(
        supersede::merge(&root, &mut active, &jobs, Some(&a))
            .unwrap()
            .is_empty()
    );
    let mut gate = wake(&b, "gate");
    assert!(
        supersede::merge(&root, &mut gate, &jobs, None)
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn crash_after_supersession_record_keeps_originals_and_recovery_is_idempotent() {
    use sha2::{Digest, Sha256};
    let root = std::env::temp_dir().join(format!(
        "kah-supersession-recovery-{}",
        kah_cli::relay_store::nonce()
    ));
    for name in ["queued", "sources", "queue-supersessions"] {
        kah_core::run_store::ensure_private_directory(&root.join(name)).unwrap();
    }
    let original = wake(&"a".repeat(40), "commit");
    let mut newer = wake(&"b".repeat(40), "commit");
    newer.trigger["superseded_triggers"] = json!([original.trigger]);
    kah_cli::observer_review::source_store::save(
        &root.join("queued"),
        "old.json",
        &serde_json::to_value(&original).unwrap(),
    )
    .unwrap();
    let digest = format!(
        "{:x}",
        Sha256::digest(fs::read(root.join("queued/old.json")).unwrap())
    );
    kah_cli::observer_review::source_store::save(&root.join("queue-supersessions"),"new.json",&json!({"state":"planned","incoming":newer,"rows":[{"original":"old.json","replacement":"new.json","digest":digest}]})).unwrap();
    supersede::recover(&root).unwrap();
    assert!(root.join("queued/new.json").exists());
    assert!(!root.join("queued/old.json").exists());
    let retained: Wake =
        kah_cli::observer_review::source_store::read(&root.join("queue-retired/old.json")).unwrap();
    assert_eq!(retained.trigger, original.trigger);
    let first = fs::read(root.join("queued/new.json")).unwrap();
    supersede::recover(&root).unwrap();
    assert_eq!(fs::read(root.join("queued/new.json")).unwrap(), first);
    fs::remove_dir_all(root).unwrap();
}
