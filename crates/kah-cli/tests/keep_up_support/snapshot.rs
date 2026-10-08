use super::*;
use std::{collections::BTreeMap, process::Command};
pub(super) fn read(root: &Path, repo: &Path) -> Value {
    read_cached(root, repo, &mut BTreeMap::new())
}
pub(super) fn read_cached(root: &Path, repo: &Path, commits: &mut BTreeMap<String, u64>) -> Value {
    let started = std::time::Instant::now();
    let mut gaps = vec![];
    let state = optional(
        &root.join("state.json"),
        &["pending", "observer", "worker"],
        &mut gaps,
    );
    let admitted = optional(
        &root.join("last-admitted-lane.json"),
        &["request", "at_ms"],
        &mut gaps,
    );
    let queued = paths(&root.join("queued"));
    let depth = if state.is_null() {
        gaps.push(json!({"stage":"state","error":"Required live state missing or unreadable"}));
        Value::Null
    } else {
        match queued {
            Ok(paths) => json!(paths.len() + usize::from(state["pending"].is_object())),
            Err(error) => {
                gaps.push(json!({"stage":"queue","error":error.to_string()}));
                Value::Null
            }
        }
    };
    let mut reviews = vec![];
    match paths(&root.join("requests")) {
        Ok(paths) => {
            for path in paths {
                match review(&path, root, repo, &state, &admitted, commits) {
                    Ok(value) => reviews.push(value),
                    Err(error) => gaps.push(json!({"path":path,"error":error.to_string()})),
                }
            }
        }
        Err(error) => gaps.push(json!({"stage":"requests","error":error.to_string()})),
    }
    let events = events(&root.join("liveness/events.jsonl"), &mut gaps);
    let retries = optional(&root.join("redelivery.json"), &["requests"], &mut gaps);
    let notices = notices(root, &mut gaps);
    json!({"at_ms":kah_observer::session::now_ms(),"read_only":true,"root":root,"repo":repo,
        "queue_depth":depth,"queued_depth":depth.as_u64().map(|d|d.saturating_sub(u64::from(state["pending"].is_object()))),
        "pending":state["pending"],"reviews":reviews,"notices":notices,"counters":counters(&events,&reviews,&retries),"gaps":gaps,
        "capture_elapsed_ms":started.elapsed().as_millis(),"snapshot_is_atomic":false})
}
fn notices(root: &Path, gaps: &mut Vec<Value>) -> Vec<Value> {
    [
        "liveness/notice.json",
        "validation-blocked.json",
        "redelivery-notice.json",
        "redelivery-reviewer-notice.json",
    ]
    .into_iter()
    .filter_map(|name| {
        let path = root.join(name);
        let value = optional(&path, &["request", "text", "state", "notice"], gaps);
        if value.is_null() {
            None
        } else {
            Some(json!({"path":name,"modified_ms":modified(&path),"value":value}))
        }
    })
    .collect()
}
fn review(
    path: &Path,
    root: &Path,
    repo: &Path,
    state: &Value,
    admitted: &Value,
    commits: &mut BTreeMap<String, u64>,
) -> Result<Value> {
    let request = request(path)?;
    let id = request["id"]
        .as_str()
        .ok_or("KEEP_UP_REQUEST stage=sample")?;
    let mut gaps = vec![];
    let timing = optional(
        &root.join("stage-timings").join(format!("{id}.json")),
        &["at_ms"],
        &mut gaps,
    );
    let mut stages = timing["at_ms"].as_object().cloned().unwrap_or_default();
    stages.insert("consumed".into(), timing["at_ms"]["completed"].clone());
    stages.insert(
        "admitted".into(),
        if admitted["request"] == id {
            admitted["at_ms"].clone()
        } else {
            Value::Null
        },
    );
    let workspace = state["observer"]["workspace"].as_str().map(Path::new);
    let ack =
        workspace.and_then(|p| modified(&p.join("outbox").join(format!("submitted-{id}.json"))));
    stages.insert("native_done".into(), json!(ack));
    let missing = [
        "queued",
        "admitted",
        "delivered",
        "native_done",
        "judged",
        "consumed",
    ]
    .into_iter()
    .filter(|key| stages.get(*key).is_none_or(Value::is_null))
    .collect::<Vec<_>>();
    let result = optional(
        &root.join("results").join(format!("{id}.json")),
        &["assessment"],
        &mut gaps,
    );
    let head = request["head"].as_str();
    let commit = if request["trigger"]["review_class"] == "commit" {
        head.and_then(|sha| commit_ms(repo, sha, commits))
    } else {
        None
    };
    Ok(
        json!({"id":id,"head":head,"purpose":request["purpose"],"review_class":request["trigger"]["review_class"],
        "commit_ms":commit,"commit_clock":"git committer timestamp, second precision","at_ms":stages,"missing_stages":missing,
        "stage_sources":{"admitted":"last-admitted-lane exact request only","native_done":"mtime of accepted submitted-request evidence ACK, not native turn end","judged":"host judgment completion","consumed":"host completed mark"},
        "assessment":result["assessment"],"gaps":gaps}),
    )
}
fn request(path: &Path) -> Result<Value> {
    let selection = Select::fields(&[
        ("id", Select::All),
        ("head", Select::All),
        ("purpose", Select::All),
        ("created_ms", Select::All),
        ("trigger", Select::fields(&[("review_class", Select::All)])),
    ]);
    let request = read_selected(path, &selection)?;
    let id = request["id"]
        .as_str()
        .ok_or("KEEP_UP_REQUEST stage=sample")?;
    if id.is_empty()
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Err("KEEP_UP_REQUEST_ID stage=sample".into());
    }
    Ok(request)
}
fn commit_ms(repo: &Path, sha: &str, commits: &mut BTreeMap<String, u64>) -> Option<u64> {
    if let Some(time) = commits.get(sha) {
        return Some(*time);
    }
    if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut command = Command::new("git");
    for key in [
        "GIT_DIR",
        "GIT_INDEX_FILE",
        "GIT_WORK_TREE",
        "GIT_COMMON_DIR",
        "GIT_PREFIX",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        command.env_remove(key);
    }
    let output = command
        .args(["-C", repo.to_str()?, "show", "-s", "--format=%ct", sha])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let time = String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?
        .checked_mul(1000)?;
    commits.insert(sha.into(), time);
    Some(time)
}
fn counters(events: &[Value], reviews: &[Value], retries: &Value) -> Value {
    let mut blocked = BTreeMap::new();
    let mut false_stalls = 0;
    for event in events {
        let sample = &event["progress"]["sample"];
        if event["state"] == "validation_blocked" {
            let key = json!([sample["request"], sample["cause"]]).to_string();
            *blocked.entry(key).or_insert(0u64) += 1;
        }
        if ["stalled", "possibly_hung"]
            .iter()
            .any(|s| event["state"] == *s)
            && reviews.iter().any(|r| {
                let ack = r["at_ms"]["native_done"].as_u64();
                let event_ms = sample["now_ms"].as_u64();
                r["id"] == sample["request"]
                    && ack.zip(event_ms).is_some_and(|(ack, t)| {
                        t >= ack && r["at_ms"]["consumed"].as_u64().is_none_or(|end| t < end)
                    })
            })
        {
            false_stalls += 1;
        }
    }
    let retries = retry_counts(retries);
    json!({"identical_blocked_repeats":blocked.values().map(|n|n.saturating_sub(1)).sum::<u64>(),"blocked_event_count":blocked.values().sum::<u64>(),
        "redelivery_attempts":retries["attempts"],"identical_redelivery_repeats":retries["identical_repeats"],
        "redelivery_binding_gaps":retries["unbound_attempts"],"false_stall_candidates":false_stalls,"liveness_events":events})
}
fn retry_counts(retries: &Value) -> Value {
    let mut contexts = BTreeMap::<String, u64>::new();
    let mut attempts = 0;
    let mut unbound = 0;
    for (request, record) in retries["requests"].as_object().into_iter().flatten() {
        for attempt in record["attempts"].as_array().into_iter().flatten() {
            attempts += 1;
            if attempt["report_digest"].is_null() || attempt["request_digest"].is_null() {
                unbound += 1;
                continue;
            }
            let key = json!([
                request,
                attempt["prior_error"],
                attempt["report_digest"],
                attempt["request_digest"]
            ])
            .to_string();
            *contexts.entry(key).or_default() += 1;
        }
    }
    json!({"attempts":attempts,"identical_repeats":contexts.values().map(|n|n.saturating_sub(1)).sum::<u64>(),"unbound_attempts":unbound})
}
