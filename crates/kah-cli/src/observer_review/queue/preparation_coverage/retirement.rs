use super::*;
use std::{fs, path::Path};

pub fn stamp(worker: &str, mut trigger: Value, intent: &Value) -> Value {
    if let Some(authority) = Authority::from_intent(intent) {
        trigger["coverage_authority"] = json!(authority);
        trigger["coverage_worker"] = worker.into();
    }
    trigger
}
pub fn run(worker: &str) -> Result<Value> {
    let root = Store::directory(worker)?;
    let _lock = Store::delivery_at(&root.join("queue-receipts"), true)?;
    let jobs = coverage_jobs(&root)?;
    if !jobs.iter().any(|(_, w)| {
        w.purpose == Purpose::Preparation && w.trigger["preparation"]["purpose"] == "drift"
    }) {
        return Ok(
            json!({"worker":worker,"before":jobs.len(),"after":jobs.len(),"retired":[],"kept":[]}),
        );
    }
    let candidates = candidates(worker, &root, &jobs)?;
    let mut retired = vec![];
    let mut kept = vec![];
    for (path, wake) in &jobs {
        if wake.purpose != Purpose::Preparation || wake.trigger["preparation"]["purpose"] != "drift"
        {
            continue;
        }
        let packet = &wake.trigger["preparation"]["state"];
        let authority = Authority::from_intent(&packet["intent"]);
        let ids = actions(&packet["event"]["actions"]);
        let by = authority
            .as_ref()
            .zip(ids.as_ref())
            .and_then(|(a, i)| candidates.iter().find(|c| covered(worker, a, i, c)));
        if let Some(by) = by {
            retired.push(retire(&root, path, by)?);
        } else {
            kept.push(json!({"record":path.file_name(),"reason":"Whole action/authority coverage not proved"}));
        }
    }
    Ok(
        json!({"worker":worker,"before":jobs.len(),"after":coverage_jobs(&root)?.len(),"retired":retired,"kept":kept}),
    )
}
fn candidates(
    worker: &str,
    root: &Path,
    jobs: &[(std::path::PathBuf, Wake)],
) -> Result<Vec<Coverage>> {
    let mut result = vec![];
    for (path, wake) in jobs.iter().filter(|(_, w)| w.purpose == Purpose::Drift) {
        if wake.trigger["coverage_worker"] == worker
            && let (Some(authority), Some(ids)) = (
                wake.trigger.get("coverage_authority"),
                actions(&wake.trigger["actions"]),
            )
        {
            result.push(Coverage {
                worker: worker.into(),
                reference: format!("queued:{}", path.file_name().unwrap().to_string_lossy()),
                authority: serde_json::from_value(authority.clone())?,
                action_ids: ids,
            });
        }
    }
    let completed: Option<Value> =
        super::super::super::state::read(&root.join("last-drift-review.json"))?;
    if let Some(id) = completed.as_ref().and_then(|c| c["request"].as_str()) {
        use crate::observer_review::source_store::read_selected;
        let selection = coverage_fields();
        let request = read_selected(
            &root.join("requests").join(format!("{id}.json")),
            &selection,
        )?;
        if request["worker"] == worker
            && let (Some(authority), Some(ids)) = (
                Authority::from_intent(&request["intent"]),
                captured_actions(&request),
            )
        {
            result.push(Coverage {
                worker: worker.into(),
                reference: format!("completed:{id}"),
                authority,
                action_ids: ids,
            });
        }
    }
    Ok(result)
}
fn captured_actions(request: &Value) -> Option<BTreeSet<String>> {
    let mut ids = actions(&request["trigger"]["actions"]).unwrap_or_default();
    for document in request["documents"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["kind"] == "worker_transcript")
    {
        ids.extend(
            document["native_records"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|r| r["context"]["record_id"].as_str().map(str::to_owned)),
        );
    }
    (!ids.is_empty()).then_some(ids)
}

fn retire(root: &Path, path: &Path, by: &Coverage) -> Result<Value> {
    let bytes = fs::read(path)?;
    let name = path.file_name().unwrap().to_str().unwrap();
    let directory = root.join("preparation-covered");
    kah_core::run_store::ensure_private_directory(&directory)?;
    let row = json!({"original":name,"digest":format!("{:x}",Sha256::digest(&bytes)),"covered_by":by,"disposition":"covered_by","at_ms":kah_observer::session::now_ms()});
    super::super::super::source_store::save(&directory, &format!("link-{name}"), &row)?;
    let archive = root.join("queue-retired");
    kah_core::run_store::ensure_private_directory(&archive)?;
    let target = archive.join(name);
    if target.exists() {
        if fs::read(&target)? != bytes {
            return Err("REVIEW_PREPARATION_COVERAGE_CHANGED stage=queue".into());
        }
        fs::remove_file(path)?;
    } else {
        fs::rename(path, target)?;
    }
    fs::File::open(archive)?.sync_all()?;
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(row)
}

fn coverage_fields() -> crate::observer_review::source_store::Select {
    use crate::observer_review::source_store::Select;
    let record = Select::fields(&[("context", Select::fields(&[("record_id", Select::All)]))]);
    let document = Select::fields(&[
        ("kind", Select::All),
        ("native_records", Select::Each(Box::new(record))),
    ]);
    let action = Select::fields(&[("id", Select::All)]);
    Select::fields(&[
        ("worker", Select::All),
        (
            "intent",
            Select::fields(&[
                ("goal", Select::All),
                ("revision", Select::All),
                ("items", Select::All),
            ]),
        ),
        (
            "trigger",
            Select::fields(&[("actions", Select::Each(Box::new(action)))]),
        ),
        ("documents", Select::Each(Box::new(document))),
    ])
}

fn coverage_jobs(root: &Path) -> Result<Vec<(std::path::PathBuf, Wake)>> {
    use crate::observer_review::source_store::{Select, read_selected};
    let authority = Select::fields(&[
        ("goal", Select::All),
        ("revision", Select::All),
        ("items", Select::All),
    ]);
    let ids = Select::Each(Box::new(Select::fields(&[("id", Select::All)])));
    let state = Select::fields(&[
        ("intent", authority),
        ("event", Select::fields(&[("actions", ids.clone())])),
    ]);
    let prep = Select::fields(&[("purpose", Select::All), ("state", state)]);
    let trigger = Select::fields(&[
        ("preparation", prep),
        ("coverage_worker", Select::All),
        ("coverage_authority", Select::All),
        ("actions", ids),
    ]);
    let selection = Select::fields(&[
        ("purpose", Select::All),
        ("turn", Select::All),
        ("trigger", trigger),
    ]);
    let mut jobs = vec![];
    let directory = root.join("queued");
    if !directory.exists() {
        return Ok(jobs);
    }
    for file in fs::read_dir(directory)? {
        let path = file?.path();
        if path.extension().is_some_and(|e| e == "json") {
            jobs.push((
                path.clone(),
                serde_json::from_value(read_selected(&path, &selection)?)?,
            ));
        }
    }
    Ok(jobs)
}
