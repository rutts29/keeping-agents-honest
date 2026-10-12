//! Real writer contention at the ordinary consume return boundary; zero agents.
use super::*;
use crate::observer_review::failure::{self, Origin};
use kah_core::{intent_history::*, projection::*, run_store::RunStore};
use std::cell::Cell;

#[test]
fn held_run_writer_keeps_report_pending_then_recovers_without_correction() {
    let worker = format!("consume-busy-{}", crate::relay_store::nonce());
    let (root, held) = setup(&worker);
    let project = root.join("project");
    let base = root.join("private-run-state");
    let report = root.join("submitted-review-1-1.json");
    let before = fs::read(&report).unwrap();
    let result = failure::validation(Origin::ReportValidation, || {
        drop(RunStore::open(&base, &project, "run")?);
        Ok(())
    });
    assert!(
        result
            .as_ref()
            .unwrap_err()
            .to_string()
            .starts_with("STORE_BUSY")
    );
    let retries = Cell::new(0);
    complete(&worker, Some("review-1-1".into()), 0, result, || {
        retries.set(retries.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(retries.get(), 1);
    assert_eq!(pending(&worker).unwrap().as_deref(), Some("review-1-1"));
    assert!(!root.join("gap.json").exists());
    assert!(!root.join("redelivery.json").exists());
    assert_eq!(fs::read(&report).unwrap(), before);
    drop(held);
    let recovered = failure::validation(Origin::ReportValidation, || {
        drop(RunStore::open(&base, &project, "run")?);
        Ok(())
    });
    complete(&worker, Some("review-1-1".into()), 0, recovered, || {
        retries.set(retries.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(retries.get(), 1);
    assert!(
        fs::read_to_string(root.join("consume-outcomes.jsonl"))
            .unwrap()
            .contains("STORE_BUSY")
    );
    fs::remove_dir_all(root.parent().unwrap()).unwrap();
}

fn setup(worker: &str) -> (std::path::PathBuf, RunStore) {
    let store = Store::open(worker).unwrap();
    let root = store.directory.clone();
    let project = root.join("project");
    fs::create_dir(&project).unwrap();
    let source = SourceRef::new(
        SourceProvider::Codex,
        worker,
        "u1",
        SourceRole::User,
        "sha256:test",
    );
    let history = History::new(GoalRevision::new(1, "Preserve every byte", vec![source])).unwrap();
    let base = root.join("private-run-state");
    let held = RunStore::create(
        &base,
        &project,
        "run",
        &history,
        &ActiveProjection::from_history(&history, ProjectionConfig::default()).unwrap(),
    )
    .unwrap();
    let state: crate::observer_review::state::State = serde_json::from_value(json!({
        "worker":{"provider":"codex","session":worker,"project":project,"pane":null,"socket":null},
        "observer":{"session":null,"pane":null,"name":"fixture","workspace":root,"model":"gpt-6.1-sol","effort":"low","transcript":null},
        "pending":{"id":"review-1-1","purpose":"checkpoint","turn":"t","head":null,"created_ms":1,"native_tokens_before":null},
        "latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false})).unwrap();
    store.save(&state).unwrap();
    let report = root.join("submitted-review-1-1.json");
    fs::write(&report, b"{\"whole\":\"Complete retained report.\\n\"}").unwrap();
    drop(store);
    (root, held)
}
