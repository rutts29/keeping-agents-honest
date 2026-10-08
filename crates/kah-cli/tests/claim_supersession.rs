//! Official git/git v2.53.0 Documentation/git-merge-base.adoc, --is-ancestor:
//! status0 proves ancestry; status1 keeps diverged or rewritten history.
use kah_cli::observer_review::{
    queue::{Wake, supersede},
    state::Purpose,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
        ])
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
fn wake(head: &str, kind: &str) -> Wake {
    Wake {
        purpose: Purpose::Checkpoint,
        turn: head.into(),
        trigger: json!({"review_class":"gate","events":[{"kind":kind,"head":head,"source":{"native_assistant_text":"Whole phase claim."}}],"user_authority_change":{"text":"Whole user words.","receipt":head}}),
    }
}
#[test]
fn descendant_claims_cover_heavy_claims_but_never_explicit_events_or_divergence() {
    let root =
        std::env::temp_dir().join(format!("claim-ancestry-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    let a = commit(&root, "base");
    let b = commit(&root, "later");
    git(&root, &["checkout", "--detach", &a]);
    let c = commit(&root, "divergent");
    let jobs = vec![
        (PathBuf::from("claim"), wake(&a, "native_phase_claim")),
        (PathBuf::from("divergent"), wake(&c, "native_phase_claim")),
        (PathBuf::from("tag"), wake(&a, "native_git_tag")),
        (PathBuf::from("gate"), wake(&a, "user_gate")),
        (PathBuf::from("completion"), wake(&a, "run_completion")),
    ];
    let mut incoming = wake(&b, "native_phase_claim");
    assert_eq!(
        supersede::merge(&root, &mut incoming, &jobs, None).unwrap(),
        vec![PathBuf::from("claim")]
    );
    assert_eq!(incoming.trigger["events"].as_array().unwrap().len(), 2);
    assert_eq!(
        incoming.trigger["superseded_triggers"][0],
        jobs[0].1.trigger
    );
    assert_eq!(
        incoming.trigger["user_authority_changes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let mut protected = wake(&b, "native_phase_claim");
    assert!(
        supersede::merge(&root, &mut protected, &jobs, Some(&a))
            .unwrap()
            .is_empty()
    );
    let mut mixed = wake(&b, "native_phase_claim");
    mixed.trigger["events"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"user_gate"}));
    assert!(
        supersede::merge(&root, &mut mixed, &jobs, None)
            .unwrap()
            .is_empty()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn enqueue_supersession_does_not_decode_unrelated_preparation_bodies() {
    use kah_cli::observer_review::{
        queue,
        state::{State, Store},
    };
    let root = std::env::temp_dir().join(format!("claim-header-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    git(&root, &["init"]);
    let a = commit(&root, "base");
    let b = commit(&root, "later");
    let worker = format!("claim-header-{}", kah_cli::relay_store::nonce());
    let store = Store::open(&worker).unwrap();
    let state:State=serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},"observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},"pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    kah_core::run_store::ensure_private_directory(&store.directory.join("queued")).unwrap();
    kah_cli::unattended_report::save(&store.directory.join("queued"),"untouched.json",&json!({"purpose":"preparation","turn":"original","trigger":{"preparation":{"purpose":"drift","state":{"$kah_source":"f".repeat(64),"encoding":"utf8"}}}})).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "old",
        wake(&a, "native_phase_claim").trigger,
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "new",
        wake(&b, "native_phase_claim").trigger,
    )
    .unwrap();
    assert_eq!(queue::header_entries(&worker).unwrap().len(), 2);
    assert!(store.directory.join("queued/untouched.json").exists());
    assert!(
        queue::entries(&worker).is_err(),
        "full readers still verify every requested original"
    );
    let state_root = store.directory.parent().unwrap().to_owned();
    drop(store);
    fs::remove_dir_all(state_root).unwrap();
    fs::remove_dir_all(root).unwrap();
}
