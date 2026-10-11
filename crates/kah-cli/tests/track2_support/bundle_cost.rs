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
    let mut skipped = vec![];
    for relative in [
        ".local/state/kah",
        ".cache/kah-observers",
        ".local/share/kah-eval",
    ] {
        let path = home.join(relative);
        if fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            continue;
        }
        walk(&path, &mut finishes, &mut skipped);
    }
    summarize(&finishes, &skipped)
}
fn summarize(finishes: &BTreeMap<String, Value>, skipped: &[Value]) -> Value {
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
        "usage_records":finishes,"vanished_paths":skipped,
        "throttle_accounting":"429/529 without usage has zero reported charge; attempts counted",
        "scope":"Deduplicate request/ticket when finished telemetry is archived; cached reuse starts no request"})
}

pub fn audited_sample(root: &Path, home: &Path) -> Value {
    let path = root.join("bundle-usage.json");
    let mut previous: BTreeMap<String, Value> = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
        Err(e) => panic!("PHASE3_USAGE_LEDGER stage=budget: {e}"),
    };
    let mut current = sample(home);
    let missing = |value: &Value| {
        previous
            .iter()
            .filter(|(id, row)| {
                row["exact_input_tokens"].is_number() && value["usage_records"][*id] != **row
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>()
    };
    if !missing(&current).is_empty() {
        // Re-scan once: archive moves can cross a directory traversal.
        current = sample(home);
    }
    let lost = missing(&current);
    for (id, row) in
        serde_json::from_value::<BTreeMap<String, Value>>(current["usage_records"].clone()).unwrap()
    {
        previous.entry(id).or_insert(row);
    }
    fs::write(&path, serde_json::to_vec_pretty(&previous).unwrap()).unwrap();
    let mut receipt = summarize(&previous, current["vanished_paths"].as_array().unwrap());
    receipt["usage_missing_unconfirmed"] = json!(lost);
    receipt["usage_record_lost"] =
        json!(!lost.is_empty() && current["vanished_paths"].as_array().unwrap().is_empty());
    receipt
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
    let receipt = audited_sample(root, home);
    fs::write(
        root.join("bundle-cost.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    (receipt["actual_jev_usd"].as_f64().unwrap() >= estimate * 2.0
        || receipt["missing_success_usage"].as_u64().unwrap() > 0
        || receipt["usage_record_lost"] == true)
        .then(|| {
            json!({"passed":false,"reason":"PHASE3_MONEY_STOP_OR_USAGE_GAP",
            "stage":"fixture_budget","estimate_usd":estimate,"stop_usd":estimate*2.0,
            "cost":receipt,"long_check_deadline":false})
        })
}

fn walk(path: &Path, rows: &mut BTreeMap<String, Value>, skipped: &mut Vec<Value>) {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if vanished(path, "metadata", &e, skipped) => return,
        Err(e) => panic!("PHASE3_COST_METADATA stage=budget: {e}"),
    };
    if meta.file_type().is_symlink() {
        return;
    }
    if meta.is_dir() {
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(e) if vanished(path, "read_dir", &e, skipped) => return,
            Err(e) => panic!("PHASE3_COST_DIRECTORY stage=budget: {e}"),
        };
        for entry in entries {
            match entry {
                Ok(entry) => walk(&entry.path(), rows, skipped),
                Err(e) if vanished(path, "entry", &e, skipped) => (),
                Err(e) => panic!("PHASE3_COST_ENTRY stage=budget: {e}"),
            }
        }
    } else if path.file_name().is_some_and(|s| s == "events.jsonl") {
        read(path, rows, skipped);
    }
}

fn read(path: &Path, rows: &mut BTreeMap<String, Value>, skipped: &mut Vec<Value>) {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if vanished(path, "open", &e, skipped) => return,
        Err(e) => panic!("PHASE3_COST_OPEN stage=budget: {e}"),
    };
    let mut input = BufReader::new(file);
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
fn vanished(path: &Path, stage: &str, error: &std::io::Error, skipped: &mut Vec<Value>) -> bool {
    if error.kind() != std::io::ErrorKind::NotFound {
        return false;
    }
    skipped.push(json!({"path":path,"stage":stage,"reason":"ENOENT","skipped":true}));
    true
}
