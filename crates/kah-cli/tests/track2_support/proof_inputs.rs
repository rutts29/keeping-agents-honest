use super::fixture::{digest, save};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
};

pub const FROZEN: &str =
    include_str!("../../../../docs/gates/evidence/phase-5o-track2-proofs/cases-frozen.json");
pub fn registration() -> Value {
    assert_eq!(
        format!("{:x}", Sha256::digest(FROZEN)),
        "fe546659493104d1763cc4627844df74f1da9aed213d415fc9c4bcaf532669b7",
        "TRACK2_CASE_CHANGED stage=freeze"
    );
    serde_json::from_str(FROZEN).unwrap()
}
pub fn retain_source(root: &Path, actual: &Path) {
    let source = actual.join(".codex/sessions/2000/01/03/rollout-2000-01-03T00-00-00-00000000-0000-4000-8000-100000000001.jsonl");
    let target = root.join("proof-source.jsonl");
    let mut output = fs::File::create(&target).unwrap();
    let mut selected = 0;
    let mut later = 0;
    for line in BufReader::new(fs::File::open(&source).unwrap()).lines() {
        let line = line.unwrap();
        let row: Value = serde_json::from_str(&line).unwrap();
        if row["timestamp"].as_str().unwrap_or("") <= "2026-10-01T13:54:00" {
            writeln!(output, "{line}").unwrap();
            selected += 1;
        } else {
            later += 1;
        }
    }
    assert!(selected > 0);
    save(
        &root.join("proof-source-audit.json"),
        &json!({"original":source,"snapshot":target,
        "sha256":digest(&target),"whole_records":selected,"later_than_snapshot_records":later,
        "scope":"All whole records through the frozen historical cutoff. Original is a live append-only builder session; no native fork or original-session mutation.","cases":registration()}),
    );
}
pub fn audit(root: &Path) -> Value {
    let path = root.join("proof-source-audit.json");
    if !path.exists() {
        return json!({"passed":true,"applicable":false});
    }
    let before: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let mut hash = Sha256::new();
    for line in
        BufReader::new(fs::File::open(before["original"].as_str().unwrap()).unwrap()).lines()
    {
        let line = line.unwrap();
        let row: Value = serde_json::from_str(&line).unwrap();
        if row["timestamp"].as_str().unwrap_or("") <= "2026-10-01T13:54:00" {
            hash.update(line.as_bytes());
            hash.update(b"\n");
        }
    }
    let original_after = format!("{:x}", hash.finalize());
    let snapshot_after = digest(&root.join("proof-source.jsonl"));
    json!({"passed":before["sha256"]==snapshot_after && snapshot_after==original_after,
        "snapshot_before":before["sha256"],"snapshot_after":snapshot_after,"historical_source_after":original_after,
        "scope":"Strict digest of all whole records through the frozen cutoff; later live appends are outside historical evaluation."})
}
