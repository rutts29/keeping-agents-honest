//! Native-run completion checkpoints retain the result and report
//! standing rules that remain unconfirmed.
use super::*;
use kah_cli::observer_review::state::{Store as Reviews, read};

/// Task rows comprise the goal and user requirements.
fn is_task(authority: &str) -> bool {
    authority == "goal" || authority.starts_with("user-")
}

pub(super) fn prove(record: &Record, run: &Path, report: &Value) -> Value {
    let session = record.native.as_ref().unwrap().session.as_deref().unwrap();
    let state = Reviews::read(session).unwrap().unwrap();
    let checkpoint = state.checkpoint.unwrap();
    assert_eq!(
        checkpoint["head"].as_str(),
        record.control.last_checkpoint()
    );
    let root = Reviews::directory(session).unwrap();
    let request = checkpoint["request"].as_str().unwrap();
    let result: Value = read(&root.join("results").join(format!("{request}.json")))
        .unwrap()
        .unwrap();
    // Run-completion checkpoints go through Option1,
    // whose archive nests the result under `latest` and has no Jev judgment.
    let case = if checkpoint["passed"] == true {
        passed(&checkpoint, &result, request);
        json!({"case":"passed"})
    } else {
        standing_rules(&checkpoint, &root, request, run, report, session)
    };
    json!({"outcome":case,"checkpoint":checkpoint,"result":result})
}

fn passed(checkpoint: &Value, result: &Value, request: &str) {
    if result["option1"] == true {
        let latest = &result["latest"];
        assert_eq!(latest["assessment"]["passed"], true);
        assert_eq!(latest["request"], request);
        assert_eq!(latest["head"], checkpoint["head"]);
    } else {
        assert_eq!(result["assessment"]["passed"], true);
        assert_eq!(result["judgment"]["backend"], "jev");
    }
}

/// `passed` is null: no row is violated, the goal and user rows are satisfied,
/// every unconfirmed row is a standing rule, and kah reported exactly those.
fn standing_rules(
    checkpoint: &Value,
    root: &Path,
    request: &str,
    run: &Path,
    report: &Value,
    session: &str,
) -> Value {
    assert!(
        checkpoint["passed"].is_null(),
        "checkpoint neither passed nor unconfirmed: {checkpoint}"
    );
    let rows = checkpoint["result"]["rows"].as_array().unwrap();
    let saved: Value = read(&root.join("requests").join(format!("{request}.json")))
        .unwrap()
        .unwrap();
    let rules = &saved["trigger"]["option1"]["input"]["rules"];
    let authority = |row: &Value| {
        rules[row["requirement"].as_str().unwrap()]["authority"]
            .as_str()
            .unwrap_or("")
            .to_owned()
    };
    assert!(rows.iter().all(|r| r["status"] != "violated"), "{rows:?}");
    let task = rows
        .iter()
        .filter(|r| is_task(&authority(r)))
        .collect::<Vec<_>>();
    assert!(!task.is_empty(), "no goal or user row: {rows:?}");
    assert!(task.iter().all(|r| r["status"] == "satisfied"), "{task:?}");
    let mut unconfirmed = rows
        .iter()
        .filter(|r| r["status"] != "satisfied")
        .inspect(|r| assert!(authority(r).starts_with("derived_rule:"), "{r}"))
        .map(|r| r["requirement"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    unconfirmed.sort();
    assert!(
        !unconfirmed.is_empty(),
        "passed is null but every row is satisfied"
    );
    assert_eq!(sorted(&report["unconfirmedStandingRules"]), unconfirmed);
    let file: Value = read(&run.join("unconfirmed-standing-rules.json"))
        .unwrap()
        .unwrap();
    assert_eq!(sorted(&file["rules"]), unconfirmed);
    assert!(
        journal(session)
            .iter()
            .any(|r| sorted(&r["rules"]) == unconfirmed)
    );
    json!({"case":"unconfirmed_standing_rules","unconfirmedStandingRules":unconfirmed,
        "taskRowsSatisfied":task.len(),"reportedInRunReport":true,"reportedInRunFile":true,
        "reportedInSessionJournal":true})
}

fn sorted(value: &Value) -> Vec<String> {
    let mut rules: Vec<String> = value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    rules.sort();
    rules
}

/// The session journal's `unconfirmed_standing_rules` records.
fn journal(session: &str) -> Vec<Value> {
    let base = RunStore::default_state_root().unwrap();
    fs::read_to_string(
        kah_observer::session::Store::directory(&base, session).join("observations.jsonl"),
    )
    .unwrap_or_default()
    .lines()
    .filter_map(|l| serde_json::from_str::<Value>(l).ok())
    .filter(|r| r["kind"] == "unconfirmed_standing_rules")
    .collect()
}
