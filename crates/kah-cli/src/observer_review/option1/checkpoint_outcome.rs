//! Unconfirmed obligations are terminal evidence, never a reason to rejudge one SHA.
use kah_observer::commit_check::{Row, Status};
use serde_json::{Value, json};

pub fn assess(input: &Value, rows: &[Row]) -> Value {
    let violations = rows
        .iter()
        .filter(|r| r.status == Status::Violated)
        .collect::<Vec<_>>();
    let unproven = rows
        .iter()
        .filter(|r| r.status == Status::Unclear)
        .collect::<Vec<_>>();
    let passed = if !violations.is_empty() {
        json!(false)
    } else if unproven.is_empty() {
        json!(true)
    } else {
        Value::Null
    };
    let non_diff = unproven.iter().filter(|r|input["rules"][&r.requirement]["literal_check"]==false)
        .map(|r|json!({"requirement":r.requirement,"literal_check":false,"status":"not_checkable_here",
            "finding":r.finding,"evidence":"Run evidence must establish this obligation; code diff alone cannot"}))
        .collect::<Vec<_>>();
    json!({"state":if passed==true{"checkpoint_passed"}else if passed==false{"checkpoint_violated"}else{"checkpoint_unconfirmed"},
        "passed":passed,"violations":violations,"unproven":unproven,"unverified_non_diff":non_diff,"terminal":true})
}
