//! Git v2.53.0 Documentation/git-merge-base.adoc --is-ancestor: 0=yes,1=no.
//! USER: queued claims may cover older ancestor claims; explicit gates/tags and
//! the active HEAD are never superseded.
use super::*;
use sha2::Digest;
use std::{path::Path, process::Command};
mod selection;
pub use selection::{claim_only, claim_trigger, trigger_head};
pub(super) fn publish(worker: &str, root: &Path, name: &str, incoming: Wake) -> Result<()> {
    publish_from(worker, root, name, incoming, None)
}
pub(super) fn republish(
    worker: &str,
    root: &Path,
    origin: &Path,
    name: &str,
    incoming: Wake,
) -> Result<()> {
    publish_from(worker, root, name, incoming, Some(origin))
}
fn publish_from(
    worker: &str,
    root: &Path,
    name: &str,
    mut incoming: Wake,
    origin: Option<&Path>,
) -> Result<()> {
    let state = crate::observer_review::state::read_control(worker)?;
    let project = state
        .as_ref()
        .map(|s| s.worker.project.clone())
        .map(Ok)
        .unwrap_or_else(|| crate::relay_store::endpoint(worker).map(|e| e.project))?;
    let active = state
        .as_ref()
        .and_then(|s| s.pending.as_ref())
        .and_then(|p| p.head.as_deref());
    let mut headers = super::headers::at(root)?;
    headers.retain(|(path, _)| origin != Some(path.as_path()));
    let jobs = selection::selected(&project, &incoming, &headers, active)?;
    let mut paths = merge(&project, &mut incoming, &jobs, active)?;
    if let Some(origin) = origin {
        paths.push(origin.to_owned());
    }
    let record = root.join("queue-supersessions");
    kah_core::run_store::ensure_private_directory(&record)?;
    let rows=paths.iter().map(|p|Ok(json!({"original":p.file_name().unwrap().to_str().unwrap(),"replacement":name,"disposition":"covered_by","digest":format!("{:x}",sha2::Sha256::digest(fs::read(p)?))}))).collect::<Result<Vec<Value>>>()?;
    super::super::source_store::save(
        &record,
        name,
        &json!({"state":"planned","incoming":incoming,"rows":rows,"at_ms":kah_observer::session::now_ms()}),
    )?;
    finish(root, name)
}
pub fn recover(root: &Path) -> Result<()> {
    let directory = root.join("queue-supersessions");
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_some_and(|s| s == "json") {
            finish(root, path.file_name().unwrap().to_str().unwrap())?;
        }
    }
    Ok(())
}
fn finish(root: &Path, name: &str) -> Result<()> {
    let directory = root.join("queue-supersessions");
    let path = directory.join(name);
    let status = super::super::source_store::read_selected(
        &path,
        &super::super::source_store::Select::fields(&[(
            "state",
            super::super::source_store::Select::All,
        )]),
    )?;
    if status["state"] == "complete" {
        return Ok(());
    }
    let mut record: Value = super::super::source_store::read(&path)?;
    let incoming: Wake = serde_json::from_value(record["incoming"].clone())?;
    ordering::reserve(root, name)?;
    metadata::queued(root, name, &incoming)?;
    if let Some(existing) = super::super::state::read::<Wake>(&root.join("queued").join(name))? {
        if serde_json::to_value(existing)? != record["incoming"] {
            return Err("REVIEW_SUPERSESSION_CHANGED stage=queue".into());
        }
    } else {
        super::super::source_store::save(&root.join("queued"), name, &record["incoming"])?;
    }
    let retired = root.join("queue-retired");
    kah_core::run_store::ensure_private_directory(&retired)?;
    for row in record["rows"]
        .as_array()
        .ok_or("REVIEW_SUPERSESSION_RECORD stage=queue")?
    {
        let old = row["original"]
            .as_str()
            .ok_or("REVIEW_SUPERSESSION_RECORD stage=queue")?;
        let target = retired.join(old);
        let source = root.join("queued").join(old);
        let present = if target.exists() { &target } else { &source };
        if row["digest"] != format!("{:x}", sha2::Sha256::digest(fs::read(present)?)) {
            return Err("REVIEW_SUPERSESSION_SOURCE_CHANGED stage=queue".into());
        }
        if !target.exists() {
            fs::rename(source, target)?;
        }
    }
    record["state"] = "complete".into();
    super::super::source_store::save(&directory, name, &record)
}
pub fn merge(
    project: &Path,
    incoming: &mut Wake,
    jobs: &[(PathBuf, Wake)],
    active_head: Option<&str>,
) -> Result<Vec<PathBuf>> {
    if incoming.trigger["review_class"] != "commit" && !claim_only(incoming) {
        return Ok(vec![]);
    }
    let new = head(incoming)?.to_owned();
    incoming.trigger["head"] = json!(new);
    let mut covered = vec![];
    let mut inherited = incoming.trigger["superseded_triggers"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut events = incoming.trigger["events"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut authority = incoming.trigger["user_authority_changes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(change) = incoming.trigger.get("user_authority_change") {
        authority.push(change.clone());
    }
    for (path, job) in jobs {
        if !selection::compatible(incoming, job) {
            continue;
        }
        let Ok(old) = head(job) else { continue };
        if active_head == Some(old) || !ancestor(project, old, &new)? {
            continue;
        }
        events.extend(
            job.trigger["events"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        authority.extend(
            job.trigger["user_authority_changes"]
                .as_array()
                .into_iter()
                .flatten()
                .cloned(),
        );
        if let Some(change) = job.trigger.get("user_authority_change") {
            authority.push(change.clone());
        }
        inherited.push(job.trigger.clone());
        covered.push(path.clone());
    }
    incoming.trigger["events"] = json!(events);
    incoming.trigger["superseded_triggers"] = json!(inherited);
    let mut distinct = vec![];
    for change in authority {
        if !distinct.contains(&change) {
            distinct.push(change);
        }
    }
    incoming.trigger["user_authority_changes"] = json!(distinct);
    Ok(covered)
}
pub fn head(wake: &Wake) -> Result<&str> {
    trigger_head(&wake.trigger)
        .filter(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("REVIEW_COMMIT_HEAD stage=supersession".into())
}
fn ancestor(project: &Path, old: &str, new: &str) -> Result<bool> {
    let status = Command::new("git")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "merge-base",
            "--is-ancestor",
            old,
            new,
        ])
        .current_dir(project)
        .status()?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("REVIEW_COMMIT_ANCESTRY stage=supersession".into()),
    }
}
