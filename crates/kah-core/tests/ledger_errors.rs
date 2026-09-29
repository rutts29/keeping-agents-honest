use kah_core::Ledger;
use std::fs;

#[test]
fn io_and_json_failures_have_distinct_codes_and_stage() {
    let path = std::env::temp_dir().join(format!("kah-ledger-errors-{}", std::process::id()));
    let missing = Ledger::open(&path).unwrap_err().to_string();
    assert!(missing.contains("LEDGER_IO stage=ledger"), "{missing}");
    fs::write(&path, "invalid json\n").unwrap();
    let invalid = Ledger::open(&path).unwrap_err().to_string();
    fs::remove_file(path).unwrap();
    assert!(invalid.contains("LEDGER_JSON stage=ledger"), "{invalid}");
}

#[test]
fn history_and_projection_errors_expose_reason_and_stage() {
    let error = kah_core::intent_history::History::from_jsonl("").unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("HISTORY_EMPTY stage=history:")
    );
    let error = kah_core::projection::ProjectionError::InvalidState {
        reason: "fixture".into(),
    };
    assert!(
        error
            .to_string()
            .starts_with("PROJECTION_STATE stage=projection:")
    );
}

#[test]
fn intake_and_storage_errors_expose_reason_and_stage() {
    let bundle = kah_core::session_bundle::SessionBundleError::EmptyRecords;
    assert!(
        bundle
            .to_string()
            .starts_with("BUNDLE_EMPTY_RECORDS stage=session_intake:")
    );
    let cursor = kah_core::run_store::SourceCursorError::EmptyRecords;
    assert!(
        cursor
            .to_string()
            .starts_with("CURSOR_EMPTY_RECORDS stage=source_cursor:")
    );
    let store = kah_core::run_store::RunStoreError::Busy;
    assert!(store.to_string().starts_with("STORE_BUSY stage=run_store:"));
}
