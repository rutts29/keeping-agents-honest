//! Private-fixture budget audit from exact provider usage, never a payload limit.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

pub fn sample(home: &Path) -> Value {
    let mut finishes = BTreeMap::new();
    for relative in [
        ".local/state/kah",
        ".cache/kah-observers",
        ".local/share/kah-eval",
    ] {
        walk(&home.join(relative), &mut finishes);
    }
    let mut tokens = 0;
    let mut statuses = BTreeMap::<String, usize>::new();
    let mut missing_success_usage = 0;
    for row in finishes.values() {
        tokens += row["exact_input_tokens"].as_u64().unwrap_or(0);
        let status = row["status"].as_u64();
        *statuses
            .entry(status.map_or("transport".into(), |s| s.to_string()))
            .or_default() += 1;
        missing_success_usage += usize::from(
            status.is_some_and(|s| (200..300).contains(&s)) && row["exact_input_tokens"].is_null(),
        );
    }
    json!({"exact_input_tokens":tokens,"actual_jev_usd":tokens as f64*0.042/1_000_000.0,
        "finished_attempts":finishes.len(),"statuses":statuses,
        "missing_success_usage":missing_success_usage,
        "throttle_accounting":"429/529 without usage has zero reported charge; attempts counted",
        "scope":"Deduplicate request/ticket when finished telemetry is archived; cached reuse starts no request"})
}

pub fn stop(root: &Path, home: &Path) -> Option<Value> {
    let estimate = std::env::var("KAH_PHASE3_BUNDLE_ESTIMATE_USD")
        .ok()?
        .parse::<f64>()
        .unwrap();
    assert!(
        estimate.is_finite() && estimate > 0.0,
        "PHASE3_ESTIMATE stage=budget"
    );
    let receipt = sample(home);
    fs::write(
        root.join("bundle-cost.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    (receipt["actual_jev_usd"].as_f64().unwrap() >= estimate * 2.0
        || receipt["missing_success_usage"].as_u64().unwrap() > 0)
        .then(|| {
            json!({"passed":false,"reason":"PHASE3_MONEY_STOP_OR_USAGE_GAP",
            "stage":"fixture_budget","estimate_usd":estimate,"stop_usd":estimate*2.0,
            "cost":receipt,"long_check_deadline":false})
        })
}

fn walk(path: &Path, rows: &mut BTreeMap<String, Value>) {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        Err(e) => panic!("PHASE3_COST_METADATA stage=budget: {e}"),
    };
    if meta.file_type().is_symlink() {
        return;
    }
    if meta.is_dir() {
        for entry in fs::read_dir(path).unwrap() {
            walk(&entry.unwrap().path(), rows);
        }
    } else if path.file_name().is_some_and(|s| s == "events.jsonl") {
        read(path, rows);
    }
}

fn read(path: &Path, rows: &mut BTreeMap<String, Value>) {
    let mut input = BufReader::new(fs::File::open(path).unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap() == 0 || !line.ends_with('\n') {
            return; // Live append tail is retained, not interpreted as a finish.
        }
        let row: Value = serde_json::from_str(&line).unwrap();
        if row["kind"] == "finish" && row["ticket"].is_string() {
            rows.insert(format!("{}:{}", row["request"], row["ticket"]), row);
        }
    }
}
