use crate::Result;
use kah_observer::{decision_units::*, review_evidence::*};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Compact {
    request_id: String,
    format: String,
    decisions: Vec<Decision>,
    #[serde(default)]
    reminder: Option<Reminder>,
    #[serde(default)]
    proportion: Option<Proportion>,
    #[serde(default)]
    steering: Option<kah_observer::steering_proposal::Proposal>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    id: String,
    name: String,
    dimension: Dimension,
    authority_items: Vec<String>,
    citations: Vec<Citation>,
    decision: SemanticDecision,
    #[serde(default)]
    topic: Option<Topic>,
    #[serde(default)]
    deferral: Option<Deferral>,
    #[serde(default)]
    controller_trigger: Option<controller_trigger::Trigger>,
    reasoning: String,
    due: bool,
    #[serde(default)]
    unconditional: bool,
    #[serde(default)]
    obligation: Option<ObligationKind>,
    #[serde(default)]
    timing_reasoning: Option<String>,
    #[serde(default)]
    timing_citations: Option<Vec<usize>>,
    #[serde(default)]
    evidence_citations: Option<Vec<usize>>,
    #[serde(default)]
    components: Vec<Component>,
    #[serde(default)]
    checks: Vec<CheckReceipt>,
    #[serde(default)]
    deferral_id: Option<String>,
    #[serde(default)]
    judgment: Option<Thinking>,
    #[serde(default)]
    existing_question_id: Option<String>,
    #[serde(default)]
    worker_question: Option<usize>,
}
impl Compact {
    pub(super) fn expand(self, version: u32) -> Result<Report> {
        if self.format != "commit-light-v2" {
            return Err("REVIEW_LIGHT_FORMAT stage=report: use commit-light-v2".into());
        }
        let mut findings = vec![];
        let mut requirements = vec![];
        for decision in self.decisions {
            findings.push(decision.finding());
            requirements.push(decision.expand());
        }
        Ok(Report {
            request_id: self.request_id,
            findings,
            reminder: self.reminder,
            decision_units: Some(ReviewUnits {
                version: Some(version),
                requirements,
                proportion: self.proportion,
                steering: self.steering,
            }),
            verification_risks: None,
            verification_contracts: None,
        })
    }
}
impl Decision {
    fn finding(&self) -> Finding {
        // Dependencies remain linked by controller-owned deferral_id in units;
        // a light review never redeclares/moves their historical anchors.
        let topic = if self.deferral.is_some() {
            Topic::Deferral
        } else {
            self.topic.clone().unwrap_or(match self.dimension {
                Dimension::Intent | Dimension::Deferral => Topic::Intent,
                Dimension::Integration => Topic::Integration,
                Dimension::Drift => Topic::Drift,
                Dimension::Steering => Topic::Steering,
            })
        };
        Finding {
            topic,
            claim: self.reasoning.clone(),
            citations: self.citations.clone(),
            deferral: self.deferral.clone(),
            checkpoint_scope: None,
        }
    }
    fn expand(self) -> Requirement {
        let all: Vec<_> = (0..self.citations.len()).collect();
        let evidence = self.evidence_citations.unwrap_or_else(|| all.clone());
        let blocker = if matches!(self.judgment, Some(Thinking::NeedsUser { .. })) {
            Some(Blocker::UserDecision)
        } else if self.decision == SemanticDecision::Deferred {
            Some(Blocker::FutureImplementation)
        } else {
            None
        };
        Requirement {
            id: self.id,
            name: self.name,
            dimension: self.dimension,
            authority_items: self.authority_items,
            citations: self.citations,
            obligation: self.obligation.map(|kind| Obligation {
                kind,
                reasoning: self.reasoning.clone(),
                evidence_citations: evidence.clone(),
            }),
            blocker,
            worker_question: self.worker_question,
            existing_question_id: self.existing_question_id,
            deferral_id: self.deferral_id,
            components: self.components,
            checks: self.checks,
            applicability: Some(Applicability {
                due: self.due,
                unconditional: self.unconditional,
                reasoning: self
                    .timing_reasoning
                    .unwrap_or_else(|| self.reasoning.clone()),
                evidence_citations: self.timing_citations.unwrap_or(all),
                controller_due: None,
                phase_completion: Some(false),
                controller_trigger: self.controller_trigger,
            }),
            judgment: self.judgment.unwrap_or(Thinking::Semantic {
                decision: self.decision,
                reasoning: self.reasoning,
                evidence_citations: evidence,
            }),
        }
    }
}
