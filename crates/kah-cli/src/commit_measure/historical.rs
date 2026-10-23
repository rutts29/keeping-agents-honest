//! Historical authority comes from retained ledgers, never scoring labels.
use super::*;
use crate::observer_review::source_store::{self, Select};
use kah_core::projection::ActiveProjection;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub projection: ActiveProjection,
    pub audit: Value,
}
struct Capture {
    path: PathBuf,
    at: u64,
    changes: Vec<(u64, u64)>,
}
pub fn load(repo: &Path, head: &str) -> Result<Snapshot> {
    let at: u64 = source::git(repo, &["show", "-s", "--format=%ct", head])?
        .trim()
        .parse::<u64>()?
        * 1000;
    let mut captures = captures(repo)?;
    captures.sort_by_key(|c| c.at.abs_diff(at));
    for capture in captures {
        // No invented maximum snapshot age. A crossing authority mutation
        // invalidates the snapshot, even when it is temporally close.
        let start = at.min(capture.at);
        let end = at.max(capture.at);
        if capture
            .changes
            .iter()
            .any(|(a, b)| *a <= end && *b >= start)
        {
            continue;
        }
        if let Some(snapshot) = ledger(&capture, at)? {
            return Ok(snapshot);
        }
    }
    Err(format!("MEASURE_HISTORICAL_INTENT_MISSING stage=corpus: {head}; no retained ledger with an unchanged authority interval (set KAH_MEASURE_WORKER to the worker session id)").into())
}
fn captures(repo: &Path) -> Result<Vec<Capture>> {
    // The retained corpus is the commits of one worker session, named by
    // KAH_MEASURE_WORKER, not every session that ever opened the same project.
    // A different worker's intent is never borrowed. Without the variable
    // there is no retained ledger to read.
    let Some(worker) = std::env::var("KAH_MEASURE_WORKER")
        .ok()
        .filter(|id| !id.is_empty())
    else {
        return Ok(vec![]);
    };
    let root = kah_core::run_store::RunStore::default_state_root()?;
    let directory = kah_observer::session::Store::directory(&root, &worker);
    let state = directory.join("state.json");
    let requests = directory.join("review/requests");
    if !state.is_file() || !requests.is_dir() {
        return Ok(vec![]);
    }
    let binding = source_store::read_selected(
        &state,
        &Select::fields(&[(
            "binding",
            Select::fields(&[("project", Select::All), ("session", Select::All)]),
        )]),
    )?;
    if binding["binding"]["project"].as_str() != repo.to_str()
        || binding["binding"]["session"] != worker
    {
        return Err("MEASURE_HISTORICAL_WORKER stage=corpus".into());
    }
    let changes = changes(&directory.join("observations.jsonl"))?;
    let mut found = vec![];
    for file in fs::read_dir(requests)? {
        let path = file?.path();
        if path.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let stamp = source_store::read_selected(
            &path,
            &Select::fields(&[("created_ms", Select::All), ("worker", Select::All)]),
        )?;
        if stamp["worker"] != worker {
            return Err("MEASURE_HISTORICAL_REQUEST_WORKER stage=corpus".into());
        }
        let at = stamp["created_ms"]
            .as_u64()
            .ok_or("MEASURE_CAPTURE_TIME stage=corpus")?;
        found.push(Capture {
            path,
            at,
            changes: changes.clone(),
        });
    }
    Ok(found)
}
fn changes(path: &Path) -> Result<Vec<(u64, u64)>> {
    let mut intervals = vec![];
    let mut previous = 0;
    let mut pending: Vec<u64> = vec![];
    for line in BufReader::new(fs::File::open(path)?).lines() {
        let row: Value = serde_json::from_str(&line?)?;
        if let Some(at) = row["at_ms"].as_u64() {
            for lower in pending.drain(..) {
                intervals.push((lower.min(at), lower.max(at)));
            }
            previous = at;
        }
        if matches!(
            row["kind"].as_str(),
            Some("source_decision_v1" | "source_review_v1")
        ) {
            // These older records lack their own timestamps: bracket each
            // mutation by adjacent timestamped records; never guess its time.
            pending.push(previous);
        }
    }
    intervals.extend(pending.into_iter().map(|lower| (lower, u64::MAX)));
    Ok(intervals)
}
fn ledger(capture: &Capture, commit_at: u64) -> Result<Option<Snapshot>> {
    let headers = source_store::read_selected(
        &capture.path,
        &Select::fields(&[(
            "documents",
            Select::Each(Box::new(Select::fields(&[
                ("kind", Select::All),
                ("id", Select::All),
            ]))),
        )]),
    )?;
    let Some(index) = headers["documents"].as_array().and_then(|docs| {
        docs.iter()
            .position(|d| d["kind"] == "ledger" && d["id"] == "ledger")
    }) else {
        return Ok(None);
    };
    let raw: Value = serde_json::from_slice(&fs::read(&capture.path)?)?;
    let root = capture
        .path
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("sources");
    let document = source_store::expand(raw["documents"][index].clone(), &root)?;
    let text = document["text"]
        .as_str()
        .ok_or("MEASURE_LEDGER_TEXT stage=corpus")?;
    let value: Value = serde_json::from_str(text)?;
    if value.is_null() {
        return Ok(None);
    }
    let projection: ActiveProjection = serde_json::from_value(value)?;
    // Prompt compaction is not authority: canonical goal/items below remain verbatim.
    Ok(Some(Snapshot {
        audit: json!({"path":capture.path,"snapshot_at_ms":capture.at,"commit_at_ms":commit_at,
        "ledger_digest":commit_check::digest(text.as_bytes()),"ledger_revision":projection.revision,
        "authority_interval":"No source_decision_v1/source_review_v1 mutation bracket overlaps snapshot-to-commit interval",
        "projection":projection}),
        projection,
    }))
}

impl Snapshot {
    pub fn always_checked(&self, id: &str) -> bool {
        use kah_core::intent_history::IntentKind;
        self.projection
            .preserve
            .iter()
            .chain(&self.projection.frontier)
            .chain(&self.projection.validation)
            .any(|item| {
                item.id.as_str() == id
                    && matches!(item.kind, IntentKind::Constraint | IntentKind::Invariant)
            })
    }
    pub fn intent(&self) -> Result<commit_check::CurrentIntent> {
        use commit_check::{Authority, CurrentIntent};
        use kah_core::intent_history::SourceRole;
        if self
            .projection
            .goal
            .source_refs
            .iter()
            .any(|s| s.role != SourceRole::User)
        {
            return Err("MEASURE_HISTORICAL_GOAL_AUTHORITY stage=resolve".into());
        }
        let mut items = BTreeMap::new();
        for item in self
            .projection
            .preserve
            .iter()
            .chain(&self.projection.frontier)
            .chain(&self.projection.validation)
        {
            if !item.is_active()
                || item
                    .introduced_by
                    .iter()
                    .any(|s| s.role != SourceRole::User)
            {
                return Err(
                    format!("MEASURE_HISTORICAL_AUTHORITY stage=resolve: {}", item.id).into(),
                );
            }
            items.insert(
                item.id.to_string(),
                Authority {
                    text: item.text.clone(),
                    origin: "user".into(),
                    sources: json!(item.introduced_by),
                },
            );
        }
        items.insert(
            "goal".into(),
            Authority {
                text: self.projection.goal.text.clone(),
                origin: "user".into(),
                sources: json!(self.projection.goal.source_refs),
            },
        );
        // Older projections have no permission-edge schema. Supply every active
        // authority as permission context, not invented grants or label rules.
        let permissions = permission_context(&items);
        Ok(CurrentIntent {
            goal: self.projection.goal.text.clone(),
            items,
            permissions,
            retired: Default::default(),
            conflicts: json!(self.projection.conflicts),
        })
    }
}

fn permission_context(
    items: &BTreeMap<String, commit_check::Authority>,
) -> BTreeMap<String, std::collections::BTreeSet<String>> {
    items
        .keys()
        .map(|id| {
            (
                id.clone(),
                items.keys().filter(|other| *other != id).cloned().collect(),
            )
        })
        .collect()
}
