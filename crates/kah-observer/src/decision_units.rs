//! The thinker decomposes facts and decides semantics. It never emits model
//! probabilities; Jev validates these units before code can apply a verdict.
use crate::review_evidence::{Citation, Document};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
mod applicability;
pub mod artifact;
mod blocker;
mod evaluation;
pub use applicability::Applicability;
pub mod completion;
pub mod context;
pub mod controller_fact;
pub mod controller_trigger;
mod literal;
mod literal_receipt;
pub use literal::Literal;
pub mod schema;
mod validation;
pub use evaluation::{Outcome, VERSION, assess, assess_with_phase_due, confirmed, questions};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewUnits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u32>,
    pub requirements: Vec<Requirement>,
    #[serde(default)]
    pub proportion: Option<Proportion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steering: Option<crate::steering_proposal::Proposal>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub name: String,
    pub dimension: Dimension,
    pub authority_items: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub obligation: Option<Obligation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocker: Option<Blocker>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_question: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub existing_question_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferral_id: Option<String>,
    pub citations: Vec<Citation>,
    pub components: Vec<Component>,
    pub checks: Vec<CheckReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applicability: Option<Applicability>,
    pub judgment: Thinking,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Obligation {
    pub kind: ObligationKind,
    pub reasoning: String,
    pub evidence_citations: Vec<usize>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObligationKind {
    Executable,
    NonExecutable,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Blocker {
    FutureImplementation,
    UserDecision,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Dimension {
    Intent,
    Integration,
    Deferral,
    Drift,
    Steering,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub name: String,
    pub role: ComponentRole,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComponentRole {
    Implementation,
    Test,
    Supporting,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckReceipt {
    pub name: String,
    pub command_citation: usize,
    pub result_citation: usize,
    pub sha_citation: usize,
    pub sha: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Thinking {
    ControllerFact {
        fact: controller_fact::Fact,
        evidence_citations: Vec<usize>,
    },
    ArtifactPreservation {
        fact: artifact::ArtifactFact,
        evidence_citations: Vec<usize>,
    },
    AtomicFact {
        fact: Literal,
        evidence_citations: Vec<usize>,
    },
    /// Historical v4 records remain readable, but native submissions reject
    /// free-form predicates: their conjunction cannot be validated mechanically.
    Fact {
        condition: String,
        evidence_citations: Vec<usize>,
    },
    Semantic {
        decision: SemanticDecision,
        reasoning: String,
        evidence_citations: Vec<usize>,
    },
    NeedsUser {
        reason: UncertaintyReason,
        summary: String,
        question: String,
        options: Vec<String>,
        evidence_citations: Vec<usize>,
        ledger_review: LedgerReview,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LedgerReview {
    pub reasoning: String,
    pub citations: Vec<usize>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticDecision {
    Satisfied,
    Violated,
    Deferred,
    NotApplicable,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyReason {
    ComplexContext,
    AmbiguousInstructions,
    ContradictoryInstructions,
    JobContext,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proportion {
    pub id: String,
    pub name: String,
    pub worst_case_consequence: String,
    pub estimated_recovery_seconds: f64,
    pub estimated_recovery_usage: String,
    pub spent_seconds: f64,
    pub spent_usage: String,
    pub reasoning: String,
    pub citations: Vec<Citation>,
}
impl ReviewUnits {
    /// Recorded legacy reports retain their actual question generation.
    pub fn version(&self) -> u32 {
        if let Some(version) = self.version {
            return version;
        }
        if self.requirements.iter().any(|r| {
            r.blocker.is_some()
                || r.worker_question.is_some()
                || r.applicability
                    .as_ref()
                    .is_some_and(|a| a.controller_due.is_some())
        }) {
            return 7;
        }
        if self.requirements.iter().any(|r| {
            r.applicability.is_some()
                || matches!(
                    r.judgment,
                    Thinking::ArtifactPreservation { .. } | Thinking::NeedsUser { .. }
                )
        }) {
            6
        } else {
            5
        }
    }
    /// Reindex complete verified units by name without removing any content.
    pub fn named_state(verified: &Value) -> Result<Value, String> {
        let requirements = verified["requirements"]
            .as_array()
            .ok_or("REVIEW_REQUIREMENT_STATE stage=decision_units")?;
        let mut named = serde_json::Map::new();
        for item in requirements {
            let id = item["id"]
                .as_str()
                .ok_or("REVIEW_REQUIREMENT_ID stage=decision_units")?;
            if !validation::id(id) || named.insert(id.into(), item.clone()).is_some() {
                return Err("REVIEW_REQUIREMENT_ID stage=decision_units".into());
            }
        }
        Ok(
            json!({"requirements":named,"proportion":verified["proportion"],"steering":verified["steering"]}),
        )
    }
    pub fn verify(&self, documents: &[Document], active_items: &[String]) -> Result<Value, String> {
        if self
            .version
            .is_some_and(|version| !(7..=VERSION).contains(&version))
        {
            return Err(
                "REVIEW_PROTOCOL_VERSION stage=decision_units: unknown report protocol".into(),
            );
        }
        let mut ids = BTreeSet::new();
        let mut covered = BTreeSet::new();
        let mut requirements = vec![];
        for requirement in &self.requirements {
            if !validation::id(&requirement.id) || !ids.insert(&requirement.id) {
                return Err("REVIEW_REQUIREMENT_ID stage=decision_units".into());
            }
            let value = validation::requirement(requirement, documents)?;
            if self.version.is_some_and(|version| version >= 7) {
                blocker::require_typed(requirement)?;
            }
            if self.version.is_some_and(|v| v >= 8) {
                literal_receipt::verify(
                    requirement,
                    value["citations"].as_array().unwrap(),
                    documents,
                )?;
            }
            if requirement
                .authority_items
                .iter()
                .any(|id| !active_items.contains(id))
            {
                return Err("REVIEW_REQUIREMENT_AUTHORITY stage=decision_units: authority item is not in the protected input".into());
            }
            covered.extend(requirement.authority_items.iter());
            requirements.push(value);
        }
        for item in active_items {
            if !covered.contains(item) {
                return Err(format!(
                    "REVIEW_REQUIREMENT_COVERAGE stage=decision_units: active item {item} has no named requirement"
                ));
            }
        }
        let proportion = self
            .proportion
            .as_ref()
            .map(|p| validation::proportion(p, documents))
            .transpose()?;
        let steering = self
            .steering
            .as_ref()
            .map(|p| p.evidence(documents))
            .transpose()?;
        Ok(json!({"requirements":requirements,"proportion":proportion,"steering":steering}))
    }
}
