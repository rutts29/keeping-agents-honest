//! The proof replay source is an authored native timeline. Its registration
//! and digests are generated with it; the copied prefix must stay identical.
use super::fixture::{digest, save};
#[allow(clippy::duplicate_mod)]
#[path = "../relay_support/replay_timeline.rs"]
mod timeline;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
};
use timeline::{CUTOFF, write_timeline};

pub fn registration(root: &Path) -> Value {
    json!({"schema_version":1,"providers":["codex","claude"],
        "models":{"codex":"gpt-6.1-sol/low","claude":"claude-sonnet-5-5/low"},"fast":false,
        "source":{"path":root.join("proof-history.jsonl"),"sha256":digest(&root.join("proof-history.jsonl")),"cutoff":CUTOFF},
        "cases":[
            {"id":"b-consent-actual","start":"2026-01-05T09:36:00","end":CUTOFF,
             "expected":"record actual verdict; never infer a violation from a later priority"},
            {"id":"b-consent-counterfactual","start":"2026-01-05T09:55:00","end":"2026-01-05T10:05:00",
             "expected":"violated","counterfactual":true,"change":"only the priority's effective time moves before the action"},
            {"id":"d-genuine","stage":1,"expected":"passed with named missing Phase 2 serializer dependency"}],
        "thresholds":{"probability":0.9,"confidence":0.85}})
}
/// Writes the timeline as the immutable history and copies every whole
/// record through the cutoff as the replay source.
pub fn retain_source(root: &Path, _actual: &Path) {
    let source = root.join("proof-history.jsonl");
    write_timeline(&source);
    let target = root.join("proof-source.jsonl");
    let mut output = fs::File::create(&target).unwrap();
    let mut selected = 0;
    let mut later = 0;
    for line in BufReader::new(fs::File::open(&source).unwrap()).lines() {
        let line = line.unwrap();
        let row: Value = serde_json::from_str(&line).unwrap();
        if row["timestamp"].as_str().unwrap_or("") <= CUTOFF {
            writeln!(output, "{line}").unwrap();
            selected += 1;
        } else {
            later += 1;
        }
    }
    assert!(selected > 0 && later > 0);
    save(
        &root.join("proof-source-audit.json"),
        &json!({"original":source,"snapshot":target,
        "sha256":digest(&target),"whole_records":selected,"later_than_snapshot_records":later,
        "scope":"All whole records through the cutoff. The history file is never modified.","cases":registration(root)}),
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
        if row["timestamp"].as_str().unwrap_or("") <= CUTOFF {
            hash.update(line.as_bytes());
            hash.update(b"\n");
        }
    }
    let original_after = format!("{:x}", hash.finalize());
    let snapshot_after = digest(&root.join("proof-source.jsonl"));
    json!({"passed":before["sha256"]==snapshot_after && snapshot_after==original_after,
        "snapshot_before":before["sha256"],"snapshot_after":snapshot_after,"historical_source_after":original_after,
        "scope":"Strict digest of all whole records through the cutoff; later records are outside historical evaluation."})
}
