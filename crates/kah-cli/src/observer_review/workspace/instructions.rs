//! The native thinker prepares facts and semantic decisions; Jev validates them.
use crate::Result;
use crate::observer_review::state::{Purpose, Request};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};

pub fn schema(purpose: &Purpose) -> Value {
    let mut value = Report::schema();
    if matches!(purpose, Purpose::Checkpoint | Purpose::Drift) {
        value["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("decision_units"));
    }
    value
}
pub fn require_units(request: &Request, report: &Report) -> Result<()> {
    if matches!(request.purpose, Purpose::Checkpoint | Purpose::Drift)
        && report.decision_units.is_none()
    {
        return Err("REVIEW_UNITS_REQUIRED stage=report: provide named factual requirements or reasoned typed semantic decisions using decision_units in schema.json".into());
    }
    Ok(())
}
pub fn decision_instructions(purpose: &Purpose) -> &'static str {
    if !matches!(purpose, Purpose::Checkpoint | Purpose::Drift) {
        return "For this steering review, provide exact evidence for the user's original input and its chronological ledger. The steering application adapter retains its existing typed questions for now.";
    }
    "Use decision_units in schema.json. You are the thinker: break the authorized work into NAMED requirements, with stable ids, exact source quotes, linked authority_items, implementation/test/supporting components and native check receipts on the claimed SHA. Cover goal and EVERY active user/relayed item; do not map a requirement to an unrelated true fact. A Fact states the exact literal condition required, with evidence_citations. For genuinely semantic understanding, DECIDE satisfied, violated, deferred or not_applicable yourself, with reasoning and cited context; Jev validates this typed decision. Never supply probabilities or an overall pass/fail verdict. Distinguish the current authorized phase from the whole future goal. Cumulative end-to-end execution must cover all available components on this commit; a future dependency needs exact authorization, current absence and evidence that it is actually necessary. Keep test scaffolding separate from the absent implementation. Use deferral_id for every controller-tracked open dependency, preserving its original anchor and phase. At checkpoints, include named intent, integration and drift requirements, covering every tracked dependency. For drift, compare actual work to current authority, separating corrected earlier failures from this stretch. When reconsideration is present, read its specific prior review document and resolve the named unproven requirements; do not just repeat the same assertion. Read pending-questions if present and keep the same requirement ids when the user answers. Before asking the user, check the existing ledger for a decision that already resolves the issue; cite and apply it instead of asking twice. Use needs_user only if you genuinely cannot decide because of complex context, ambiguous or contradictory instructions, or job context: provide a named question, summary, options and ledger_review with exact ledger citations. Only the affected decision waits; unrelated authorized work continues. For a side problem, include proportion: estimate its worst-case consequence and recovery time/usage, record the time/usage already spent with citations, and reason from the authorization that existed THEN. Jev decides proportion, not a new time cap. A proposed reminder stays concise with exact decisive user quotes and the full-record pointer; never dump the projection or truncate. Reminders reach the worker only after Jev validates a violation or steering. Remain idle after acknowledged submission."
}
