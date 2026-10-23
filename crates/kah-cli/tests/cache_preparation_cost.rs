//! Retained host usage receipts; no model turn or provider call.
use kah_cli::observer_review::source_store::{self, Select};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires externally supplied observer review data and an output path"]
fn retained_preparation_time_and_token_share() {
    let root = PathBuf::from(std::env::var_os("KAH_C_COST_REVIEW_ROOT").unwrap());
    let output = PathBuf::from(std::env::var_os("KAH_C_COST_OUTPUT").unwrap());
    let since = std::env::var("KAH_C_COST_SINCE_MS")
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let rows = rows(&root, since);
    let prep: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["purpose"]
                .as_str()
                .is_some_and(|p| p.starts_with("preparation"))
        })
        .collect();
    let sum =
        |items: &[&Value], key: &str| -> u64 { items.iter().filter_map(|r| r[key].as_u64()).sum() };
    let all: Vec<_> = rows.iter().collect();
    let result = json!({"read_only":true,"since_ms":since,"completed_reviews":rows.len(),"preparation_reviews":prep.len(),
        "elapsed_ms_sum":sum(&all,"elapsed_ms"),"preparation_elapsed_ms_sum":sum(&prep,"elapsed_ms"),
        "native_tokens_sum":if all.iter().all(|r|r["native_tokens"].is_u64()){json!(sum(&all,"native_tokens"))}else{Value::Null},"preparation_native_tokens_sum":if prep.iter().all(|r|r["native_tokens"].is_u64()){json!(sum(&prep,"native_tokens"))}else{Value::Null},
        "scope":"Completed retained request intervals only. Pending work and native overhead outside bound intervals are excluded and explicitly unmeasured; summed durations may overlap.","rows":rows,"acceptance":false});
    fs::write(output, serde_json::to_vec_pretty(&result).unwrap()).unwrap();
}

fn rows(root: &std::path::Path, since: u64) -> Vec<Value> {
    let mut rows = vec![];
    for entry in fs::read_dir(root.join("stage-timings")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let timing: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let Some(done) = timing["at_ms"]["completed"]
            .as_u64()
            .filter(|t| *t >= since)
        else {
            continue;
        };
        let id = timing["request"].as_str().unwrap();
        let request = source_store::read_selected(
            &root.join("requests").join(format!("{id}.json")),
            &Select::fields(&[
                ("created_ms", Select::All),
                ("purpose", Select::All),
                ("trigger", Select::fields(&[("review_class", Select::All)])),
            ]),
        )
        .unwrap();
        let result_path = root.join("results").join(format!("{id}.json"));
        let result = if result_path.exists() {
            source_store::read_selected(
                &result_path,
                &Select::fields(&[
                    ("native_tokens", Select::All),
                    ("judgment", Select::fields(&[("metrics", Select::All)])),
                ]),
            )
            .unwrap()
        } else {
            Value::Null
        };
        rows.push(json!({"request":id,"created_ms":request["created_ms"],"purpose":request["purpose"],
            "review_class":request["trigger"]["review_class"],"completed_ms":done,
            "elapsed_ms":timing["at_ms"]["admitted"].as_u64().map(|start| done.saturating_sub(start)),
            "native_tokens":result["native_tokens"],"usage_gap":if result.is_null(){json!("No archived preparation usage; token attribution unmeasured")}else{Value::Null},
            "jev":result["judgment"]["metrics"]}));
    }
    rows
}
