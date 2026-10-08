use super::*;
use crate::review_evidence::verify_quote;
pub(super) fn verified_requirement(
    r: &Requirement,
    documents: &[Document],
    sources: &crate::review_evidence::VerifiedSources<'_>,
    version: Option<u32>,
    authority_required: bool,
) -> Result<Value, String> {
    (|| {
        let value = requirement(r, documents, sources, authority_required)?;
        if version.is_some_and(|v| v >= 7) {
            super::blocker::require_typed(r)?;
        }
        if version.is_some_and(|v| v >= 8) {
            super::literal_receipt::verify(r, value["citations"].as_array().unwrap(), documents)?;
        }
        Ok(value)
    })()
    .map_err(|e: String| format!("{e}; requirement {:?}", r.id))
}

pub(super) fn id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
pub(super) fn requirement(
    r: &Requirement,
    documents: &[Document],
    sources: &crate::review_evidence::VerifiedSources<'_>,
    authority_required: bool,
) -> Result<Value, String> {
    required_fields(r, authority_required)?;
    let citations = r
        .citations
        .iter()
        .enumerate()
        .map(|(i, c)| sources.quote(c).map_err(|e| format!("{e}; citations[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    let indices = thinking(&r.judgment)?;
    super::blocker::validate(r, &citations)?;
    if let Some(a) = &r.applicability {
        if let Some(fact) = &a.controller_due {
            if !a.due || a.unconditional {
                return Err("REVIEW_COMPLETION_RECEIPT_TIMING stage=decision_units: receipt is for this event's phase completion requirements".into());
            }
            fact.verify(r, documents)?;
        }
        if let Some(trigger) = &a.controller_trigger {
            trigger.verify(r, documents)?;
        }
        applicability::validate(a, &citations)?;
    }
    if indices.is_empty() || indices.iter().any(|i| citations.get(*i).is_none()) {
        return Err("REVIEW_REQUIREMENT_CITATION stage=decision_units: judgment.evidence_citations must select existing citations".into());
    }
    if r.components.iter().any(|c| c.name.trim().is_empty()) {
        return Err(
            "REVIEW_REQUIREMENT_COMPONENT stage=decision_units: components[].name must be nonempty"
                .into(),
        );
    }
    for (index, receipt) in r.checks.iter().enumerate() {
        check(receipt, &citations)
            .map_err(|e| format!("{e}; checks[{index}] {:?}", receipt.name))?;
    }
    if let Thinking::ArtifactPreservation { fact, .. } = &r.judgment {
        fact.verify(r, documents)?;
    }
    controller_fact(r, documents)?;
    obligation(r, &citations)?;
    if let Thinking::AtomicFact {
        fact,
        evidence_citations,
    } = &r.judgment
    {
        fact.validate(&r.checks, evidence_citations)?;
    }
    ledger_review(r, &citations)?;
    let mut value =
        serde_json::to_value(r).map_err(|_| "REVIEW_REQUIREMENT_JSON stage=decision_units")?;
    value["citations"] = json!(citations);
    Ok(value)
}
fn obligation(r: &Requirement, citations: &[Value]) -> Result<(), String> {
    if let Some(o) = &r.obligation {
        nonempty(&o.reasoning, "obligation.reasoning")?;
        if o.evidence_citations.is_empty()
            || o.evidence_citations
                .iter()
                .any(|i| citations.get(*i).is_none())
        {
            return Err("REVIEW_OBLIGATION_CITATION stage=decision_units: obligation.evidence_citations must select existing citations".into());
        }
    }
    Ok(())
}
fn controller_fact(r: &Requirement, documents: &[Document]) -> Result<(), String> {
    if let Thinking::ControllerFact {
        fact,
        evidence_citations,
    } = &r.judgment
    {
        if !evidence_citations.contains(&fact.receipt_citation) {
            return Err("REVIEW_CONTROLLER_FACT_CITATION stage=decision_units: judgment.evidence_citations must include fact.receipt_citation".into());
        }
        fact.verify(r, documents)?;
    }
    Ok(())
}
fn thinking(judgment: &Thinking) -> Result<&[usize], String> {
    let indices = match judgment {
        Thinking::ControllerFact {
            evidence_citations, ..
        } => evidence_citations,
        Thinking::ArtifactPreservation {
            evidence_citations, ..
        } => evidence_citations,
        Thinking::AtomicFact {
            evidence_citations, ..
        } => evidence_citations,
        Thinking::Fact {
            condition,
            evidence_citations,
        } => {
            nonempty(condition, "judgment.condition")?;
            evidence_citations
        }
        Thinking::Semantic {
            reasoning,
            evidence_citations,
            ..
        } => {
            nonempty(reasoning, "judgment.reasoning")?;
            evidence_citations
        }
        Thinking::NeedsUser {
            summary,
            question,
            options,
            evidence_citations,
            ..
        } => {
            nonempty(summary, "judgment.summary")?;
            nonempty(question, "judgment.question")?;
            if options.len() < 2 || options.iter().any(|s| s.trim().is_empty()) {
                return Err(
                    "REVIEW_USER_OPTIONS stage=decision_units: provide meaningful alternatives"
                        .into(),
                );
            }
            evidence_citations
        }
    };
    Ok(indices)
}
fn nonempty(text: &str, field: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err(format!(
            "REVIEW_THINKING_EMPTY stage=decision_units: {field} is empty"
        ));
    }
    Ok(())
}
fn check(receipt: &CheckReceipt, citations: &[Value]) -> Result<(), String> {
    nonempty(&receipt.name, "checks[].name")?;
    if ![
        receipt.command_citation,
        receipt.result_citation,
        receipt.sha_citation,
    ]
    .iter()
    .all(|i| citations.get(*i).is_some())
        || !matches!(receipt.sha.len(), 40 | 64)
        || !receipt.sha.bytes().all(|c| c.is_ascii_hexdigit())
        || !citations[receipt.sha_citation]["quote"]
            .as_str()
            .is_some_and(|q| q.contains(&receipt.sha))
    {
        return Err("REVIEW_CHECK_RECEIPT stage=decision_units: command/result/SHA must have exact source citations".into());
    }
    for index in [receipt.command_citation, receipt.result_citation] {
        if !native_check(&citations[index], index == receipt.result_citation) {
            return Err(
                "REVIEW_CHECK_PROVENANCE stage=decision_units: native receipt provenance missing"
                    .into(),
            );
        }
    }
    Ok(())
}
fn native_check(citation: &Value, result: bool) -> bool {
    citation["kind"] == "worker_transcript"
        && citation["native_records"]
            .as_array()
            .is_some_and(|records| {
                records.iter().any(|r| {
                    r["record_type"] == "commandExecution"
                        || (if result {
                            r["record_type"] == "user"
                                && r["content_types"]
                                    .as_array()
                                    .is_some_and(|types| types.iter().any(|t| t == "tool_result"))
                        } else {
                            r["record_type"] == "assistant"
                                && r["content_types"]
                                    .as_array()
                                    .is_some_and(|types| types.iter().any(|t| t == "tool_use"))
                        })
                })
            })
}
pub(super) fn proportion(p: &Proportion, documents: &[Document]) -> Result<Value, String> {
    if !id(&p.id)
        || p.estimated_recovery_seconds < 0.0
        || p.spent_seconds < 0.0
        || !p.estimated_recovery_seconds.is_finite()
        || !p.spent_seconds.is_finite()
        || p.citations.is_empty()
    {
        return Err("REVIEW_PROPORTION_EVIDENCE stage=decision_units".into());
    }
    for (field, text) in [
        ("proportion.name", &p.name),
        (
            "proportion.worst_case_consequence",
            &p.worst_case_consequence,
        ),
        (
            "proportion.estimated_recovery_usage",
            &p.estimated_recovery_usage,
        ),
        ("proportion.spent_usage", &p.spent_usage),
        ("proportion.reasoning", &p.reasoning),
    ] {
        nonempty(text, field)?;
    }
    let citations = p
        .citations
        .iter()
        .map(|c| verify_quote(c, documents))
        .collect::<Result<Vec<_>, _>>()?;
    let mut value =
        serde_json::to_value(p).map_err(|_| "REVIEW_PROPORTION_JSON stage=decision_units")?;
    value["citations"] = json!(citations);
    Ok(value)
}

fn required_fields(r: &Requirement, authority_required: bool) -> Result<(), String> {
    let empty = [
        ("name", r.name.trim().is_empty()),
        (
            "authority_items",
            authority_required && r.authority_items.is_empty(),
        ),
        ("citations", r.citations.is_empty()),
    ]
    .into_iter()
    .filter_map(|(field, missing)| missing.then_some(field))
    .collect::<Vec<_>>();
    if !empty.is_empty() {
        return Err(format!(
            "REVIEW_REQUIREMENT_EMPTY stage=decision_units: requirement {:?}, empty fields: {}",
            r.id,
            empty.join(", ")
        ));
    }
    Ok(())
}

fn ledger_review(r: &Requirement, citations: &[Value]) -> Result<(), String> {
    if let Thinking::NeedsUser { ledger_review, .. } = &r.judgment {
        nonempty(&ledger_review.reasoning, "judgment.ledger_review.reasoning")?;
        if ledger_review.citations.is_empty()
            || ledger_review
                .citations
                .iter()
                .any(|i| !citations.get(*i).is_some_and(|c| c["kind"] == "ledger"))
        {
            return Err("REVIEW_LEDGER_CHECK stage=decision_units: cite existing ledger decisions before asking the user".into());
        }
    }
    Ok(())
}
