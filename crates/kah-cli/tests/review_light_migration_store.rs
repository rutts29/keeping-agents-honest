//! Native Git ancestry: git/git v2.53.0 Documentation/git-merge-base.adoc.
use kah_cli::observer_review::{
    queue::{self, migration},
    state::{Purpose, State, Store},
};
use serde_json::json;
use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn commit(root: &Path, text: &str) -> String {
    fs::write(root.join("file"), text).unwrap();
    git(root, &["add", "file"]);
    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            text,
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}
fn fixture() -> (String, std::path::PathBuf, String) {
    let worker = format!("migration-{}", kah_cli::relay_store::nonce());
    let root = std::env::temp_dir().join(&worker);
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["config", "user.name", "Fixture"]);
    git(&root, &["config", "user.email", "fixture@example.invalid"]);
    let a = commit(&root, "base");
    let b = commit(&root, "next");
    let store = Store::open(&worker).unwrap();
    let state:State = serde_json::from_value(json!({"worker":{"provider":"codex","session":worker,"project":root,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":{"id":"keep-active","purpose":"drift","turn":"active","head":null,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    queue::enqueue(&worker,Purpose::Checkpoint,"old",json!({"events":[{"kind":"native_head","head":a}],"user_authority_change":{"receipt":"whole-original"}})).unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "real-gate",
        json!({"events":[{"kind":"user_gate","head":a}]}),
    )
    .unwrap();
    queue::enqueue(
        &worker,
        Purpose::Checkpoint,
        "new",
        json!({"review_class":"commit","head":b,"events":[{"kind":"native_head","head":b}]}),
    )
    .unwrap();
    queue::enqueue(&worker,Purpose::Checkpoint,"unknown-head",
        json!({"review_class":"commit","head":null,"events":[{"kind":"native_phase_claim","source":"Whole unknown-SHA claim"}]})).unwrap();
    drop(store);
    (worker, root, b)
}
#[test]
fn migration_retires_exact_originals_collapses_ancestors_and_preserves_real_gates() {
    let (worker, root, b) = fixture();
    let before = queue::entries(&worker).unwrap();
    let old = before
        .iter()
        .find(|(_, w)| w.turn == "old")
        .unwrap()
        .0
        .clone();
    let bytes = fs::read(&old).unwrap();
    let first = migration::run(&worker, "user-approved").unwrap();
    let after = queue::entries(&worker).unwrap();
    assert_eq!(after.len(), 3);
    assert!(
        after
            .iter()
            .any(|(_, w)| w.turn == "real-gate" && w.trigger["review_class"].is_null())
    );
    let light = after
        .iter()
        .find(|(_, w)| w.trigger["review_class"] == "commit" && w.trigger["head"] == b)
        .unwrap();
    assert_eq!(light.1.trigger["head"], b);
    assert_eq!(
        light.1.trigger["events"].as_array().unwrap().len(),
        2,
        "no duplicate self-events"
    );
    assert_eq!(
        light.1.trigger["user_authority_changes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let directory = Store::directory(&worker).unwrap();
    assert_eq!(
        fs::read(
            directory
                .join("queue-retired")
                .join(old.file_name().unwrap())
        )
        .unwrap(),
        bytes
    );
    assert_eq!(
        Store::read(&worker).unwrap().unwrap().pending.unwrap().id,
        "keep-active"
    );
    assert_eq!(migration::run(&worker, "user-approved").unwrap(), first);
    resume_planned(&worker, &first);
    fs::remove_dir_all(directory.parent().unwrap()).unwrap();
    fs::remove_dir_all(root).unwrap();
}
fn resume_planned(worker: &str, manifest: &serde_json::Value) {
    let directory = Store::directory(worker).unwrap().join("queue-migrations");
    let record = fs::read_dir(&directory)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut interrupted = manifest.clone();
    interrupted["state"] = "planned".into();
    kah_cli::observer_review::source_store::save(
        &directory,
        record.file_name().unwrap().to_str().unwrap(),
        &interrupted,
    )
    .unwrap();
    assert_eq!(
        migration::run(worker, "user-approved").unwrap()["state"],
        "complete"
    );
    let jobs = queue::entries(worker).unwrap();
    assert_eq!(jobs.len(), 3);
    assert_eq!(
        jobs.iter()
            .find(|(_, w)| w.trigger["review_class"] == "commit" && !w.trigger["head"].is_null())
            .unwrap()
            .1
            .trigger["events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
