use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use kah_core::intent_history::{
    Event, GoalRevision, History, IntentId, IntentItem, IntentKind, SourceProvider, SourceRef,
    SourceRole,
};
use kah_core::projection::{ActiveProjection, project};
use kah_core::run_store::{
    RunId, RunStore, RunStoreError, SourceCursor, SourceCursorComparison, SourceRecordDigest,
    repository_id,
};

fn temporary_directory(case: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after the Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "kah_core-run-store-{case}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&path).expect("create isolated test directory");
    path
}

fn source(record: &str) -> SourceRef {
    SourceRef::new(
        SourceProvider::Bundle,
        "private-session",
        record,
        SourceRole::User,
        format!("sha256:{record}"),
    )
}

fn history() -> History {
    History::new(GoalRevision::new(
        1,
        "persist the run",
        vec![source("goal")],
    ))
    .expect("start history")
}

fn item(id: &str, record: &str) -> IntentItem {
    IntentItem::new(
        IntentId::new(id),
        IntentKind::Constraint,
        format!("requirement {id}"),
        vec![source(record)],
    )
}

fn projection(history: &History) -> ActiveProjection {
    project(history, 16 * 1024).expect("project history")
}

fn digest(hex: char) -> String {
    format!("sha256:{}", hex.to_string().repeat(64))
}

fn source_cursor(records: &[(&str, char)]) -> SourceCursor {
    SourceCursor::new(
        SourceProvider::Bundle,
        "private-session-locator",
        digest('f'),
        records
            .iter()
            .map(|(id, hex)| SourceRecordDigest::new(*id, digest(*hex)))
            .collect(),
    )
    .expect("build source cursor")
}

fn create_store(case: &str) -> (PathBuf, PathBuf, RunStore, History) {
    let root = temporary_directory(case);
    let repository = root.join("repository");
    fs::create_dir(&repository).expect("create repository");
    let history = history();
    let store = RunStore::create(
        &root,
        &repository,
        "run_01",
        &history,
        &projection(&history),
    )
    .expect("create run store");
    (root, repository, store, history)
}

fn pending_record(history: &History, id: &str) -> kah_core::intent_history::EventRecord {
    kah_core::intent_history::EventRecord::new(
        u64::try_from(history.records().len() + 1).expect("sequence fits"),
        Event::RequirementAdded {
            item: item(id, &format!("source-{id}")),
            approval: None,
        },
    )
}

#[path = "run_store/cases0.rs"]
mod cases0;
#[path = "run_store/cases1.rs"]
mod cases1;
