//! Fresh native submissions use v8; historical reports remain readable.
use crate::Result;
use crate::observer_review::state::Request;
use kah_observer::{
    decision_units::{Blocker, SemanticDecision, Thinking},
    review_evidence::Report,
};

pub(super) fn require(request: &Request, report: &Report) -> Result<()> {
    let Some(units) = &report.decision_units else {
        return Ok(());
    };
    if units.version != Some(8) {
        return Err(
            "REVIEW_PROTOCOL_REQUIRED stage=report: fresh reports need decision_units.version=8"
                .into(),
        );
    }
    let literal = request
        .documents
        .iter()
        .any(|d| d.kind == "controller_completion_due");
    for r in &units.requirements {
        match r.judgment {
            Thinking::NeedsUser{..} if r.blocker!=Some(Blocker::UserDecision) => return Err("REVIEW_BLOCKER_USER_QUESTION stage=report: declare user_decision and use the named question path".into()),
            Thinking::Semantic{decision:SemanticDecision::Deferred,..} if r.blocker!=Some(Blocker::FutureImplementation) => return Err("REVIEW_BLOCKER_FUTURE stage=report: semantic deferred only describes authorized future implementation".into()),
            _=>(),
        }
        if let Some(a) = &r.applicability {
            let phase=a.phase_completion.ok_or("REVIEW_APPLICABILITY_SCOPE stage=report: declare phase_completion for the currently reviewed phase, not the whole future goal")?;
            if phase && literal && a.controller_due.is_none() {
                return Err("REVIEW_COMPLETION_RECEIPT_REQUIRED stage=report: copy this phase's completion timing receipt from completion-due".into());
            }
            if !phase && a.controller_due.is_some() {
                return Err("REVIEW_COMPLETION_RECEIPT_SCOPE stage=report: ongoing work does not use a phase-completion receipt".into());
            }
        }
    }
    Ok(())
}
pub(super) const CONTEXT: &str = "V8 evidence: each independent decision receives ONLY its own checked source manifest. For judgment.evidence_citations select whole relevant citations for the substantive decision; for applicability.evidence_citations select whole relevant timing/authority citations. Rust keeps the goal and linked authority_items verbatim and evaluates these decisions separately; no individual decision is split. A semantic requirement's checks must have all their command/result/SHA citations in its substantive selection; omit unrelated checks instead of putting another requirement's failure in this manifest. Preserve complete quotes, their meaning and source locations. The complete report still covers every protected active user item. Exact sha/exit_status/output_line facts are checked mechanically against trusted native metadata/output; contradictions are rejected, matching receipts cannot become violations. Never invent a missing native exit status or infer it from command text. For a current pending question, copy its existing_question_id from pending-questions and keep that requirement id. One decision has one active question. The worker's exact native quote refines the referenced wait, retaining the old wording as superseded lineage, never opening another wait. Unknown, answered or unrelated references are rejected. With no existing wait set existing_question_id=null. A later answer keeps the referenced requirement and cites the new user authority; never treat the observer's paraphrase as the user's answer.";
pub(super) const INSTRUCTIONS: &str = "V8 contract: set decision_units.version=8. Each requirement declares blocker=null when unblocked, future_implementation only for missing implementation explicitly authorized in a later phase, or user_decision for a missing user choice. A user_decision MUST use needs_user, a cited ledger_review and one named question; never hide it as semantic deferred. If the worker already asked natively, copy its exact native assistant quote into question and put that citation index in worker_question; the relay identifies it as the worker's question, never a second phrasing. Preserve the same requirement id when answered. Only the affected decision waits; unrelated authorized work continues. Each non-needs_user requirement's applicability declares phase_completion=true only for completion obligations of the CURRENT phase at this event, not the entire future goal. If completion-due exists, those phase_completion requirements copy controller_due={receipt_citation,receipt} from that controller document, due=true, unconditional=false. Rust establishes that timing literally; Jev validates the substantive decision. Unconfirmed phase claims have no such receipt and keep semantic applicability. For ongoing/mid-turn or later-phase timing use phase_completion=false, controller_due=null and reasoned semantic applicability. Unconditional prohibitions always apply: due=true, unconditional=true, phase_completion=false. A missing future completion receipt is not a mid-turn violation. The controller binds receipts to this request, worker, event and SHA. Never fabricate a receipt or reuse an old one. Keep exact source quotes, source digests and native provenance. No probabilities or overall pass/fail from the thinker.";
