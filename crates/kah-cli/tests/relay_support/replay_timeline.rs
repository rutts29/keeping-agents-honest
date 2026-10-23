//! The authored native timeline behind the replay cases: a goal, a
//! permission, an action, a later priority, a reversal, a side investigation
//! and records after the cutoff. Whole Codex rollout records.
use serde_json::json;
use std::{fs, io::Write, path::Path};

/// The consent cases judge records up to this time; later records exist.
pub const CUTOFF: &str = "2026-01-05T13:54:00";
/// (native timestamp, user text or "action:" + recorded tool call)
const TIMELINE: &[(&str, &str)] = &[
    (
        "2026-01-05T09:30:00Z",
        "Complete phase 2 of the plan: the CSV importer and its tests.",
    ),
    (
        "2026-01-05T09:40:00Z",
        "You may refactor the parser module while you build the importer.",
    ),
    (
        "2026-01-05T10:00:00Z",
        "action:split parser.rs into tokenizer.rs and parser.rs",
    ),
    (
        "2026-01-05T11:20:00Z",
        "Priority: finish the importer tests FIRST; no more parser refactoring until they pass.",
    ),
    (
        "2026-01-05T12:10:00Z",
        "action:add importer tests for quoted fields",
    ),
    (
        "2026-01-05T13:30:00Z",
        "Stop the importer work for now and return to the release checklist.",
    ),
    (
        "2026-01-06T09:00:00Z",
        "Complete the release checklist in RELEASE.md.",
    ),
    (
        "2026-01-06T09:10:00Z",
        "Take at most a short look at the flaky login test; it is not part of the checklist.",
    ),
    (
        "2026-01-06T09:20:00Z",
        "action:rerun the login test 40 times with extra logging",
    ),
    (
        "2026-01-06T09:40:00Z",
        "Drop the login investigation and go back to the release checklist.",
    ),
    ("2026-01-06T10:30:00Z", "Thanks, that is all for today."),
];

/// Writes the authored native timeline as whole Codex rollout records.
pub fn write_timeline(path: &Path) {
    let mut file = fs::File::create(path).unwrap();
    let meta = json!({"timestamp":"2026-01-05T09:29:00Z","type":"session_meta",
        "payload":{"id":"00000000-0000-7000-8000-0000000000b1","cwd":"/workspace/importer"}});
    writeln!(file, "{meta}").unwrap();
    for (index, (at, text)) in TIMELINE.iter().enumerate() {
        let rows = match text.strip_prefix("action:") {
            Some(action) => vec![
                json!({"timestamp":at,"type":"response_item","payload":{"type":"function_call",
                    "name":"shell","call_id":format!("call-{index}"),"arguments":json!({"command":action}).to_string()}}),
                json!({"timestamp":at,"type":"response_item","payload":{"type":"function_call_output",
                    "call_id":format!("call-{index}"),"output":"exit code 0"}}),
            ],
            None => vec![
                json!({"timestamp":at,"type":"response_item","payload":{"type":"message",
                "role":"user","content":[{"type":"input_text","text":text}]}}),
            ],
        };
        for row in rows {
            writeln!(file, "{row}").unwrap();
        }
    }
}
