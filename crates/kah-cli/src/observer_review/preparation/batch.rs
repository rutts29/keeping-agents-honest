//! One native thinker wake; every selection still belongs to one whole decision.
use super::{Job, Manifest};
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod lifecycle;
pub mod packet_refs;
pub mod packets;
mod planning;
pub use planning::plan;
mod selection;
mod transport;
mod validation;
pub use lifecycle::*;
pub use transport::*;
pub use validation::validate_iter;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub id: String,
    pub review: String,
    pub worker: String,
    pub decisions: Vec<Entry>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub decision: String,
    pub digest: String,
}
impl Batch {
    pub fn new(review: &str, worker: &str, jobs: &[Job]) -> Result<Self> {
        if !valid_id(review) {
            return Err(
                "PREPARATION_BATCH_IDENTITY stage=context_preparation: invalid review id".into(),
            );
        }
        let mut seen = BTreeSet::new();
        let mut decisions = vec![];
        for job in jobs {
            if job.worker != worker || !valid_id(&job.decision) || !seen.insert(&job.decision) {
                return Err(format!(
                    "PREPARATION_BATCH_IDENTITY stage=context_preparation: {}",
                    job.decision
                )
                .into());
            }
            decisions.push(Entry {
                decision: job.decision.clone(),
                digest: digest(job)?,
            });
        }
        Self::from_entries(review, worker, decisions)
    }
    pub(crate) fn from_entries(review: &str, worker: &str, decisions: Vec<Entry>) -> Result<Self> {
        if decisions.is_empty() {
            return Err("PREPARATION_BATCH_EMPTY stage=context_preparation".into());
        }
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(review, worker, &decisions))?)
        );
        Ok(Self {
            id: format!("{review}-batch-{digest}"),
            review: review.into(),
            worker: worker.into(),
            decisions,
        })
    }
}
fn digest(job: &Job) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(job)?)))
}
pub fn validate(batch: &Batch, jobs: &[Job], manifests: &[Manifest]) -> Result<Vec<Manifest>> {
    let expected = Batch::new(&batch.review, &batch.worker, jobs)?;
    if expected.id != batch.id
        || serde_json::to_value(&expected.decisions)? != serde_json::to_value(&batch.decisions)?
    {
        return Err("PREPARATION_BATCH_CHANGED stage=context_preparation".into());
    }
    super::super::failure::validation(super::super::failure::Origin::SelectionValidation, || {
        let mut by_id = BTreeMap::new();
        for manifest in manifests {
            if by_id.insert(manifest.decision.as_str(), manifest).is_some() {
                return Err(format!(
                    "CONTEXT_BATCH_DUPLICATE stage=context_selection: {}",
                    manifest.decision
                )
                .into());
            }
        }
        let mut checked = vec![];
        for job in jobs {
            let manifest = by_id.remove(job.decision.as_str()).ok_or_else(|| {
                format!(
                    "CONTEXT_BATCH_MISSING stage=context_selection: {}",
                    job.decision
                )
            })?;
            checked.push(super::validate_selection(
                job,
                &serde_json::to_vec(manifest)?,
            )?);
        }
        if !by_id.is_empty() {
            return Err(format!(
                "CONTEXT_BATCH_UNKNOWN stage=context_selection: {:?}",
                by_id.keys()
            )
            .into());
        }
        Ok(checked)
    })
}

fn valid_id(id: &str) -> bool {
    id.starts_with("review-") && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
