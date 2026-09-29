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
