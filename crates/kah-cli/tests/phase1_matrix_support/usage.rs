use super::*;
pub fn snapshot(home: &Path, project: &Path) -> Value {
    let mut files = vec![];
    collect(&home.join(".codex/sessions"), &mut files);
    for path in files {
        let mut id = None;
        let mut usage = Value::Null;
        let mut model = Value::Null;
        let mut effort = Value::Null;
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let mut matches = false;
        for line in text.lines() {
            let Ok(row) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if row["type"] == "session_meta" {
                matches = row["payload"]["cwd"] == project.to_string_lossy().as_ref();
                id = row["payload"]["id"].as_str().map(str::to_owned);
            }
            if row["type"] == "turn_context" {
                model = row["payload"]["model"].clone();
                effort = row["payload"]["effort"].clone();
            }
            if row["type"] == "event_msg" && row["payload"]["type"] == "token_count" {
                usage = row["payload"]["info"]["total_token_usage"].clone();
            }
        }
        if matches && id.is_some() {
            return json!({"session":id,"transcript":path,"usage":usage,"model":model,"effort":effort});
        }
    }
    Value::Null
}
fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|x| x == "jsonl") {
            files.push(path);
        }
    }
}
pub fn delta(before: &Value, after: &Value) -> Value {
    let fields = [
        "input_tokens",
        "cached_input_tokens",
        "output_tokens",
        "reasoning_output_tokens",
        "total_tokens",
    ];
    Value::Object(
        fields
            .into_iter()
            .map(|key| {
                (
                    key.into(),
                    json!(
                        after["usage"][key]
                            .as_u64()
                            .unwrap_or(0)
                            .saturating_sub(before["usage"][key].as_u64().unwrap_or(0))
                    ),
                )
            })
            .collect(),
    )
}

pub fn resources(root: &Path) -> Value {
    let df = Command::new("df")
        .args(["-Pk", root.to_str().unwrap()])
        .output()
        .unwrap();
    let line = String::from_utf8(df.stdout).unwrap();
    let fields = line
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .collect::<Vec<_>>();
    let mem = fs::read_to_string("/proc/meminfo").unwrap();
    let values = mem
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            let number = value.split_whitespace().next()?.parse::<u64>().ok()?;
            Some((key.to_owned(), json!(number * 1024)))
        })
        .collect::<serde_json::Map<_, _>>();
    json!({"tmpfs_used_bytes":fields[2].parse::<u64>().unwrap()*1024,"tmpfs_free_bytes":fields[3].parse::<u64>().unwrap()*1024,"memory":values})
}
