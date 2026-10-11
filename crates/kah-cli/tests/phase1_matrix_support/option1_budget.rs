//! Money/disk operating guard, never a content or native-turn deadline.
use super::*;
pub fn check(f: &fixture::Fixture) -> Value {
    let mut files = vec![];
    collect(&f.home.join(".local/state/kah"), &mut files);
    let mut tokens = 0u64;
    let mut physical = 0;
    for path in files {
        let text = fs::read_to_string(path).unwrap();
        for line in text.lines() {
            if let Ok(row) = serde_json::from_str::<Value>(line) {
                if row["kind"] == "start" {
                    physical += 1;
                }
                if row["kind"] == "finish" {
                    tokens += row["exact_input_tokens"].as_u64().unwrap_or(0);
                }
            }
        }
    }
    let usd = tokens as f64 * 0.042 / 1_000_000.0;
    let estimate: Value = serde_json::from_slice(
        &fs::read(std::env::var_os("KAH_OPTION1_ESTIMATE").unwrap()).unwrap(),
    )
    .unwrap();
    let stop = estimate["estimated_usd"].as_f64().unwrap() * 4.0;
    let prior = estimate["prior_actual_usd"].as_f64().unwrap_or(0.0);
    let result = json!({"exact_input_tokens":tokens,"actual_usd":usd,"prior_actual_usd":prior,"aggregate_actual_usd":usd+prior,"physical_starts":physical,"stop_usd":stop,"unknown_usage":"rejected/unfinished calls have no reported input-token usage"});
    save(&f.evidence.join("jev-spend-current.json"), &result);
    assert!(
        usd + prior <= stop,
        "STEP5_SPEND_STOP stage=fixture: {result}"
    );
    let resource = usage::resources(&f.root);
    save(&f.evidence.join("resources-current.json"), &resource);
    let df = Command::new("df")
        .args(["-Pk", f.evidence.to_str().unwrap()])
        .output()
        .unwrap();
    let text = String::from_utf8(df.stdout).unwrap();
    let free = text
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .nth(3)
        .unwrap()
        .parse::<u64>()
        .unwrap()
        * 1024;
    assert!(
        free >= 4 * 1024 * 1024 * 1024,
        "STEP5_DISK_STOP stage=fixture: volume below4GiB; evidence retained"
    );
    result
}
fn collect(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.file_name().is_some_and(|n| n == "events.jsonl") {
            files.push(path);
        }
    }
}
