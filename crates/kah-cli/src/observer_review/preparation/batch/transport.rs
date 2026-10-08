//! Batch publication uses the same immutable source store and selection checks.
use super::*;
use crate::observer_review::{
    source_store,
    state::{self, Store},
};
use serde_json::json;
use std::path::Path;
pub fn stage(worker: &str, review: &str, jobs: &[Job]) -> Result<Batch> {
    stage_iter(worker, review, jobs.iter().cloned().map(Ok))
}
pub fn stage_iter(
    worker: &str,
    review: &str,
    jobs: impl IntoIterator<Item = Result<Job>>,
) -> Result<Batch> {
    let root = Store::directory(worker)?;
    let mut entries = vec![];
    let mut seen = std::collections::BTreeSet::new();
    for job in jobs {
        let job = job?;
        let batch = Batch::new(review, worker, std::slice::from_ref(&job))?;
        let entry = batch.decisions.into_iter().next().unwrap();
        if !seen.insert(entry.decision.clone()) {
            return Err(
                "PREPARATION_BATCH_IDENTITY stage=context_preparation: duplicate job".into(),
            );
        }
        store_job(&root, &job)?;
        entries.push(entry);
    }
    let batch = Batch::from_entries(review, worker, entries)?;
    source_store::save(
        &root,
        &format!("preparation-batch-{}.json", batch.id),
        &serde_json::to_value(&batch)?,
    )?;
    crate::unattended_report::save(
        &root,
        "preparation-current.json",
        &json!({"review":review,"decision":batch.id}),
    )?;
    Ok(batch)
}
pub fn load(worker: &str, id: &str) -> Result<Option<Batch>> {
    state::read(&Store::directory(worker)?.join(format!("preparation-batch-{id}.json")))
}
pub fn jobs(worker: &str, batch: &Batch) -> Result<Vec<Job>> {
    batch
        .decisions
        .iter()
        .map(|entry| load_job(worker, entry))
        .collect()
}
pub fn load_job(worker: &str, entry: &Entry) -> Result<Job> {
    let root = Store::directory(worker)?;
    let job: Job = state::read(&root.join(format!("preparation-batch-job-{}.json", entry.digest)))?
        .ok_or("PREPARATION_BATCH_JOB stage=context_preparation")?;
    if digest(&job)? != entry.digest || job.decision != entry.decision || job.worker != worker {
        return Err(format!(
            "PREPARATION_BATCH_CHANGED stage=context_preparation: {}",
            entry.decision
        )
        .into());
    }
    Ok(job)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Submission {
    pub batch: String,
    pub selections: Vec<Manifest>,
}
pub fn submit(args: &[String]) -> Result<()> {
    if args.len() != 6 || args[2] != "--request" || args[4] != "--json" {
        return Err("PREPARATION_BATCH_ARGUMENTS stage=context_preparation".into());
    }
    let path = Path::new(&args[3]);
    let batch: Batch = source_store::read(path)?;
    let root = path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or("PREPARATION_BATCH_PATH stage=context_preparation")?;
    let input = crate::observer_review::consume::read_report(Path::new(&args[5]))?;
    let jobs = batch.decisions.iter().map(|entry| {
        source_store::read(
            &path
                .parent()
                .unwrap()
                .join(format!("preparation-{}.json", entry.decision)),
        )
    });
    let (submitted, manifests) = match validate_submission_iter(&batch, jobs, &input) {
        Ok(result) => result,
        Err(error) => {
            crate::observer_review::liveness::selection_rejection::record_value(
                &root.join("outbox"),
                &batch.id,
                &serde_json::to_value(&batch)?,
                Path::new(&args[5]),
                &input,
                error.as_ref(),
            )?;
            return Err(error);
        }
    };
    for manifest in &manifests {
        crate::observer_review::workspace::publish_output(
            &root.join("outbox"),
            &format!("selected-{}.json", manifest.decision),
            &serde_json::to_value(manifest)?,
        )?;
    }
    // This final marker is published only after EVERY manifest validated.
    crate::observer_review::workspace::publish_output(
        &root.join("outbox"),
        &format!("selected-{}.json", batch.id),
        &serde_json::to_value(&submitted)?,
    )?;
    println!(
        "{}",
        json!({"decision":batch.id,"status":"context_selection_submitted","decisions":manifests.len()})
    );
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/observer_unit/preparation_batch_transport.rs"]
mod tests;

pub fn validate_submission(
    batch: &Batch,
    jobs: &[Job],
    bytes: &[u8],
) -> Result<(Submission, Vec<Manifest>)> {
    crate::observer_review::failure::validation(
        crate::observer_review::failure::Origin::SelectionValidation,
        || {
            let submitted: Submission = serde_json::from_slice(bytes)
                .map_err(|e| format!("CONTEXT_BATCH_JSON stage=context_selection: {e}"))?;
            if submitted.batch != batch.id {
                return Err("CONTEXT_BATCH_IDENTITY stage=context_selection".into());
            }
            let manifests = validate(batch, jobs, &submitted.selections)?;
            Ok((submitted, manifests))
        },
    )
}

fn store_job(root: &Path, job: &Job) -> Result<()> {
    let snapshot = format!("preparation-batch-job-{}.json", digest(job)?);
    if let Some(saved) = state::read::<Job>(&root.join(&snapshot))? {
        if digest(&saved)? != digest(job)? {
            return Err(
                "PREPARATION_BATCH_CHANGED stage=context_preparation: immutable job snapshot"
                    .into(),
            );
        }
    } else {
        source_store::save(root, &snapshot, &serde_json::to_value(job)?)?;
    }
    if let Some(saved) =
        state::read::<Job>(&root.join(format!("preparation-{}.json", job.decision)))?
        && digest(&saved)? != digest(job)?
    {
        return Err(format!(
            "PREPARATION_BATCH_CHANGED stage=context_preparation: {}",
            job.decision
        )
        .into());
    }
    source_store::save(
        root,
        &format!("preparation-{}.json", job.decision),
        &serde_json::to_value(job)?,
    )?;
    Ok(())
}

pub fn validate_submission_iter(
    batch: &Batch,
    jobs: impl IntoIterator<Item = Result<Job>>,
    bytes: &[u8],
) -> Result<(Submission, Vec<Manifest>)> {
    crate::observer_review::failure::validation(
        crate::observer_review::failure::Origin::SelectionValidation,
        || {
            let submitted: Submission = serde_json::from_slice(bytes)
                .map_err(|e| format!("CONTEXT_BATCH_JSON stage=context_selection: {e}"))?;
            if submitted.batch != batch.id {
                return Err("CONTEXT_BATCH_IDENTITY stage=context_selection".into());
            }
            let checked = validate_iter(batch, jobs, &submitted.selections)?;
            Ok((submitted, checked))
        },
    )
}
