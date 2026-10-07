use serde_json::{Value, json};
use std::collections::BTreeSet;
type Check<T> = Result<T, String>;

pub fn memory(rows: &[Value], finding: &str) -> Check<Value> {
    let fail = || {
        "TRACK2_MEMORY stage=acceptance: three checkpoints, identity, usage or continuity absent"
            .to_string()
    };
    if rows.len() != 3 || finding.is_empty() {
        return Err(fail());
    }
    let mut ids = BTreeSet::new();
    for row in rows {
        if row["purpose"] != "checkpoint"
            || row["observer_session"].as_str().is_none()
            || row["pane"].as_str().is_none()
            || row["observer_session"] != rows[0]["observer_session"]
            || row["pane"] != rows[0]["pane"]
            || !row["native_tokens"].as_u64().is_some_and(|n| n > 0)
            || !row["usage_gap"].is_null()
            || row["judgment"]["backend"] != "jev"
            || !row["judgment"]["metrics"]["calls"]
                .as_u64()
                .is_some_and(|n| n > 0)
            || !row["request"].as_str().is_some_and(|id| ids.insert(id))
        {
            return Err(fail());
        }
    }
    if !earlier_citation(&rows[2]["report"], finding) {
        return Err(fail());
    }
    Ok(
        json!({"passed":true,"same_session":rows[0]["observer_session"],
        "same_pane":rows[0]["pane"],"checkpoints":ids,"earlier_finding":finding}),
    )
}

fn earlier_citation(value: &Value, finding: &str) -> bool {
    match value {
        Value::Object(fields) => {
            fields
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|s| s == "previous-review" || s.starts_with("historical-review"))
                && fields
                    .get("quote")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.contains(finding))
                || fields.values().any(|v| earlier_citation(v, finding))
        }
        Value::Array(items) => items.iter().any(|v| earlier_citation(v, finding)),
        _ => false,
    }
}

pub fn latency(commits: &[Value]) -> Check<Value> {
    if commits.is_empty() {
        return Err("TRACK2_LATENCY_EMPTY stage=measurement".into());
    }
    let mut times = Vec::new();
    let mut shas = BTreeSet::new();
    for commit in commits {
        let (Some(sha), Some(start), Some(end)) = (
            commit["sha"].as_str(),
            commit["committed_ms"].as_u64(),
            commit["verdict_ms"].as_u64(),
        ) else {
            return Err("TRACK2_LATENCY_MISSING stage=measurement".into());
        };
        if !shas.insert(sha) || end < start {
            return Err("TRACK2_LATENCY_ORDER stage=measurement".into());
        }
        times.push(end - start);
    }
    times.sort_unstable();
    let p50 = times[(times.len() * 50).div_ceil(100) - 1];
    let p90 = times[(times.len() * 90).div_ceil(100) - 1];
    Ok(
        json!({"n":times.len(),"method":"nearest_rank","p50_ms":p50,"p90_ms":p90,
        "passed":p50<=900000 && p90<=1800000,"durations_ms":times}),
    )
}

pub fn drain(start: usize, end: usize, completed: usize, from: u64, to: u64) -> Check<Value> {
    if completed == 0 || to <= from {
        return Err("TRACK2_DRAIN_EMPTY stage=measurement".into());
    }
    Ok(
        json!({"passed":end<=start,"depth_start":start,"depth_end":end,
        "completed":completed,"operating_ms":to-from,
        "completed_reviews_per_hour":completed as f64*3600000.0/(to-from) as f64}),
    )
}
pub fn perturbations(receipts: &Value) -> Check<Value> {
    let restart = &receipts["observer-restart"];
    let swap = &receipts["binary-swap"];
    let ack = &receipts["post-ack"];
    let host = &receipts["host-restart"];
    let writers = &receipts["concurrent"];
    let passed = restart["session_before"].is_string()
        && restart["session_before"] == restart["session_after"]
        && restart["pid_before"].is_u64()
        && restart["pid_after"].is_u64()
        && restart["pid_before"] != restart["pid_after"]
        && swap["mid_review"] == true
        && swap["before_sha256"].is_string()
        && swap["after_sha256"].is_string()
        && swap["before_sha256"] != swap["after_sha256"]
        && ack["actual_native_ack"] == true
        && ack["redelivery_unchanged"] == true
        && ack["progress"]["stalled"] == false
        && ack["progress"]["sample"]["controller_active"] == true
        && ack["end_ms"]
            .as_u64()
            .zip(ack["start_ms"].as_u64())
            .is_some_and(|(end, start)| end >= start && end - start >= 20000)
        && host["fixture_owned"] == true
        && host["observer_session"] == restart["session_after"]
        && host["host_pid_before"].is_u64()
        && host["host_pid_after"].is_u64()
        && host["host_pid_before"] != host["host_pid_after"]
        && writers["statuses"]
            .as_array()
            .is_some_and(|s| s.len() == 3 && s.iter().all(|v| v == true));
    if !passed {
        return Err("TRACK2_PERTURBATION_RECEIPT stage=acceptance: missing or unproven restart/swap/ACK/writer condition".into());
    }
    Ok(json!({"passed":true,"receipts":receipts}))
}
