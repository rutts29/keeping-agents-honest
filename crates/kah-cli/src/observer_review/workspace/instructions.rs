//! The native thinker prepares facts and semantic decisions; Jev validates them.
use crate::Result;
use crate::observer_review::state::{Purpose, Request};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
#[path = "v7.rs"]
mod v7;

pub fn schema(purpose: &Purpose) -> Value {
    let mut value = Report::schema();
    if matches!(
        purpose,
        Purpose::Checkpoint | Purpose::Drift | Purpose::Steering
    ) {
        value["required"]
            .as_array_mut()
            .unwrap()
            .push(json!("decision_units"));
    }
    value
}
pub fn require_units(request: &Request, report: &Report) -> Result<()> {
    if report.decision_units.as_ref().is_some_and(|u| {
        u.requirements.iter().any(|r| {
            matches!(
                r.judgment,
                kah_observer::decision_units::Thinking::Fact { .. }
            )
        })
    }) {
        return Err("REVIEW_ATOMIC_FACT_REQUIRED stage=report: split each SHA, exit status and output line into its own atomic_fact; decide semantic meaning separately".into());
    }
    if matches!(
        request.purpose,
        Purpose::Checkpoint | Purpose::Drift | Purpose::Steering
    ) && report.decision_units.is_none()
    {
        return Err("REVIEW_UNITS_REQUIRED stage=report: provide named factual requirements or reasoned typed semantic decisions using decision_units in schema.json".into());
    }
    v7::require(request, report)?;
    if report.decision_units.as_ref().is_some_and(|u| {
        u.requirements.iter().any(|r| {
            r.applicability.is_none()
                && !matches!(
                    r.judgment,
                    kah_observer::decision_units::Thinking::NeedsUser { .. }
                )
        })
    }) {
        return Err("REVIEW_APPLICABILITY_REQUIRED stage=report: decide whether each obligation is due at this event from exact authority and native progress; unconditional prohibitions always apply".into());
    }
    if request.purpose == Purpose::Steering
        && report.decision_units.as_ref().is_some_and(|u| {
            u.steering.is_none()
                && !u.requirements.iter().any(|r| {
                    matches!(
                        r.judgment,
                        kah_observer::decision_units::Thinking::NeedsUser { .. }
                    )
                })
        })
    {
        return Err("REVIEW_STEERING_PROPOSAL stage=report: decide the typed steering operation, or ask a genuine unresolved user question".into());
    }
    Ok(())
}
pub fn decision_instructions(purpose: &Purpose) -> String {
    if matches!(
        purpose,
        Purpose::Steering | Purpose::Checkpoint | Purpose::Drift
    ) {
        return format!("{}\n{}", v7::INSTRUCTIONS, base_instructions(purpose));
    }
    base_instructions(purpose).into()
}
fn base_instructions(purpose: &Purpose) -> &'static str {
    if *purpose == Purpose::Steering {
        return "Use decision_units and its steering object from schema.json for semantic steering. Think through the user's ORIGINAL uncertain_user_input and the host's steering_context. Only prior_intent is historical authorization for typing this original input; newer directives are a separate compatibility check, never earlier priors. Decide one typed operation: directs (whether the message gives direction), kind, supersedes (an exact older item id or null), source_record copied EXACTLY from uncertain_user_input.source.record, and reasoning with exact citations. Use question_only or none with directs=false for no new directive; replacement targets apply only to correction, scope_reduction or goal_change. Decide conflicts=true/false individually for EVERY id in steering_context.newer, with reasoning and citations. Explicit receipt links are handled by the controller. No probabilities; Jev validates your decisions before the unchanged user words enter history. For every requirement decide applicability: due at this event, unconditional, reasoning and exact authority/native progress citation indices. Unconditional user prohibitions always apply. Semantic timing is validated by Jev; literal phase timing and unconditional prohibitions follow the v7 contract. Also give named steering requirements covering every active authority item in intent.json. If you genuinely cannot decide, use needs_user with a named question, options and cited ledger_review: check existing ledger decisions first and do not ask what is already answered. Keep unrelated work available. If a side problem is involved, include proportion with cited worst-case recovery and spent time/usage estimates under the then-effective authorization. Propose a concise reminder with decisive exact user quotes and a full-record pointer, never a dumped projection or truncation; Claude's complete delivered string must fit its native 10000 UTF-16 units. Stay idle after acknowledged submission.";
    }
    if !matches!(purpose, Purpose::Checkpoint | Purpose::Drift) {
        return "Read the request-specific preparation instructions and use their schema.";
    }
    "Use decision_units in schema.json. You are the thinker: break the authorized work into NAMED requirements, with stable ids, exact source quotes, linked authority_items, implementation/test/supporting components and native check receipts on the claimed SHA. For every named requirement decide applicability first: due at THIS event, unconditional, reasoning and exact authority plus native progress citation indices. Unconditional user prohibitions apply at every event. A promised completion receipt is not due merely because work is still mid-turn; distinguish a real completion claim from ongoing authorized work. Semantic applicability is validated by Jev; use the v7 controller receipt for current-phase completion timing. Only an applicable confirmed violation can produce a reminder. Cover goal and EVERY active user/relayed item; do not map a requirement to an unrelated true fact. For preserved tests or plan use artifact_preservation with a controller_artifact_digests citation; copy its exact path, baseline_blob, delivered_blob and delivered_sha receipt. Rust compares the blob digests directly. Never infer byte equality from a broad semantic claim or fabricate a missing pre-work baseline. An atomic_fact checks exactly one native receipt property: sha, exit_status, or output_line, using its check index and expected value. Give every SHA, status and pass line its own named requirement; code composes them. Free-form fact conditions are rejected. Composition correctness, intent and closure are semantic decisions with reasoning and exact evidence, never hidden inside a compound factual condition. For genuinely semantic understanding, DECIDE satisfied, violated, deferred or not_applicable yourself, with reasoning and cited context; Jev validates this typed decision. Never supply probabilities or an overall pass/fail verdict. Distinguish the current authorized phase from the whole future goal. Cumulative end-to-end execution must cover all available components on this commit; a future dependency needs exact authorization, current absence and evidence that it is actually necessary. Keep test scaffolding separate from the absent implementation. Use deferral_id for every controller-tracked open dependency, preserving its original anchor and phase. At checkpoints, include named intent, integration and drift requirements, covering every tracked dependency. For drift, compare actual work to current authority, separating corrected earlier failures from this stretch. When reconsideration is present, read its specific prior review document and resolve the named unproven requirements; do not just repeat the same assertion. Read pending-questions if present and keep the same requirement ids when the user answers. Before asking the user, check the existing ledger for a decision that already resolves the issue; cite and apply it instead of asking twice. Use needs_user only if you genuinely cannot decide because of complex context, ambiguous or contradictory instructions, or job context: provide a named question, summary, options and ledger_review with exact ledger citations. Only the affected decision waits; unrelated authorized work continues. For a side problem, include proportion: estimate its worst-case consequence and recovery time/usage, record the time/usage already spent with citations, and reason from the authorization that existed THEN. Jev decides proportion, not a new time cap. A proposed reminder stays concise with exact decisive user quotes and the full-record pointer; never dump the projection or truncate. Reminders reach the worker only after Jev validates a violation or steering. Remain idle after acknowledged submission."
}
