//! Source-bound whole records and estimated readiness. No provider or native calls.
use crate::{
    context_budget::{Calibration, Plan},
    context_selection::digest,
    judge::Questions,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
mod records;
mod selection;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
    pub body_digest: String,
    pub sources: Vec<Source>,
}
/// Host-created source document; archive provenance is verified by the caller.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub target: String,
    pub original_digest: String,
    pub document: Value,
    pub document_digest: String,
    pub records: Vec<Record>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub id: String,
    pub pointer: String,
    pub digest: String,
    pub dependencies: BTreeSet<String>,
    pub required: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Choice {
    pub id: String,
    pub digest: String,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub body_digest: String,
    pub catalogue_digest: String,
    pub selected: Vec<Choice>,
    pub excluded: Vec<Choice>,
}
#[derive(Default)]
pub struct Policy {
    pub calibration: Option<Calibration>,
    /// Canonical parsed-body digests of requests rejected as oversized.
    pub known_oversized: BTreeSet<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "route", rename_all = "snake_case")]
pub enum Outcome {
    Rendered {
        body: Value,
        estimate: Plan,
        body_digest: String,
    },
    DirectObserverReview {
        gap: Gap,
    },
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Gap {
    pub reason: String,
    pub original_body_digest: String,
    pub candidate_body_digest: String,
    pub estimate: Option<Plan>,
    pub selected_records: Vec<String>,
    pub question_ids: Vec<String>,
}
pub fn catalogue_digest(catalogue: &Catalogue) -> String {
    digest(&serde_json::to_value(catalogue).expect("catalogue serialization"))
}
pub fn prepare(
    body: &Value,
    catalogue: &Catalogue,
    manifest: &Manifest,
    policy: &Policy,
) -> Result<Outcome, String> {
    let known = records::validate(body, catalogue)?;
    let selected = selection::validate(catalogue, manifest, &known)?;
    let candidate = records::render(body, catalogue, &selected)?;
    let candidate_digest = digest(&candidate);
    let questions: Questions = serde_json::from_value(candidate["questions"].clone())
        .map_err(|_| "PREPARATION_QUESTIONS stage=whole_preparation")?;
    crate::judge::validate_request(&candidate["state"], &questions)
        .map_err(|error| error.to_string())?;
    let plan = policy
        .calibration
        .as_ref()
        .and_then(|c| c.plan(&candidate["state"], &questions));
    let reason = if policy.known_oversized.contains(&candidate_digest) {
        Some("known_oversized")
    } else if plan.as_ref().is_none_or(|p| p.prepare) {
        Some(if plan.is_none() {
            "calibration_unavailable"
        } else {
            "whole_items_exceed_budget"
        })
    } else {
        None
    };
    if let Some(reason) = reason {
        return Ok(Outcome::DirectObserverReview {
            gap: Gap {
                reason: reason.into(),
                original_body_digest: catalogue.body_digest.clone(),
                candidate_body_digest: candidate_digest,
                estimate: plan,
                selected_records: selected.into_iter().map(str::to_owned).collect(),
                question_ids: questions.keys().cloned().collect(),
            },
        });
    }
    Ok(Outcome::Rendered {
        body: candidate,
        estimate: plan.unwrap(),
        body_digest: candidate_digest,
    })
}
