use super::*;
use crate::review_evidence::verify_quote;

pub(super) fn id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}
pub(super) fn requirement(r: &Requirement, documents: &[Document]) -> Result<Value, String> {
    if r.name.trim().is_empty() || r.authority_items.is_empty() || r.citations.is_empty() {
        return Err("REVIEW_REQUIREMENT_EMPTY stage=decision_units".into());
    }
    let citations = r
        .citations
        .iter()
        .map(|c| verify_quote(c, documents))
        .collect::<Result<Vec<_>, _>>()?;
    let indices = thinking(&r.judgment)?;
    if indices.is_empty() || indices.iter().any(|i| citations.get(*i).is_none()) {
        return Err("REVIEW_REQUIREMENT_CITATION stage=decision_units".into());
    }
    if r.components.iter().any(|c| c.name.trim().is_empty()) {
        return Err("REVIEW_REQUIREMENT_COMPONENT stage=decision_units".into());
    }
    for receipt in &r.checks {
        check(receipt, &citations)?;
    }
    if let Thinking::NeedsUser { ledger_review, .. } = &r.judgment {
        nonempty(&ledger_review.reasoning)?;
        if ledger_review.citations.is_empty()
            || ledger_review
                .citations
                .iter()
                .any(|i| !citations.get(*i).is_some_and(|c| c["kind"] == "ledger"))
        {
            return Err("REVIEW_LEDGER_CHECK stage=decision_units: cite existing ledger decisions before asking the user".into());
        }
    }
    let mut value =
        serde_json::to_value(r).map_err(|_| "REVIEW_REQUIREMENT_JSON stage=decision_units")?;
    value["citations"] = json!(citations);
    Ok(value)
}
fn thinking(judgment: &Thinking) -> Result<&[usize], String> {
    let indices = match judgment {
        Thinking::Fact {
            condition,
            evidence_citations,
        } => {
            nonempty(condition)?;
            evidence_citations
        }
        Thinking::Semantic {
            reasoning,
            evidence_citations,
            ..
        } => {
            nonempty(reasoning)?;
            evidence_citations
        }
        Thinking::NeedsUser {
            summary,
            question,
            options,
            evidence_citations,
            ..
        } => {
            nonempty(summary)?;
            nonempty(question)?;
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
fn nonempty(text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("REVIEW_THINKING_EMPTY stage=decision_units".into());
    }
    Ok(())
}
fn check(receipt: &CheckReceipt, citations: &[Value]) -> Result<(), String> {
    nonempty(&receipt.name)?;
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
    for text in [
        &p.name,
        &p.worst_case_consequence,
        &p.estimated_recovery_usage,
        &p.spent_usage,
        &p.reasoning,
    ] {
        nonempty(text)?;
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
