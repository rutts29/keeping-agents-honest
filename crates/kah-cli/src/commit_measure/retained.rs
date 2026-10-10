//! Adapter for existing source-verified light/v12 findings; no generated facts.
use super::*;
use kah_observer::{
    commit_check::{CodeQuote, FindingKind, Row, Status},
    review_evidence::Report,
};
pub fn rows(report: &Report, input: &Input) -> Vec<Row> {
    report
        .decision_units
        .as_ref()
        .into_iter()
        .flat_map(|u| &u.requirements)
        .filter_map(|r| {
            let (status, finding) = match &r.judgment {
                kah_observer::decision_units::Thinking::Semantic {
                    decision,
                    reasoning,
                    ..
                } => {
                    let status = match decision {
                        kah_observer::decision_units::SemanticDecision::Satisfied => {
                            Status::Satisfied
                        }
                        kah_observer::decision_units::SemanticDecision::Violated => {
                            Status::Violated
                        }
                        _ => Status::Unclear,
                    };
                    (status, reasoning.clone())
                }
                _ => return None,
            };
            let quotes = quotes(&r.citations, input);
            Some(Row {
                requirement: r.id.clone(),
                finding,
                status,
                kind: FindingKind::Behavior,
                quotes,
            })
        })
        .collect()
}

fn quotes(citations: &[kah_observer::review_evidence::Citation], input: &Input) -> Vec<CodeQuote> {
    citations
        .iter()
        .filter_map(|citation| {
            input.files.iter().find_map(|(path, file)| {
                if file.digest != citation.digest {
                    return None;
                }
                let start = file
                    .text
                    .match_indices(&citation.quote)
                    .nth(citation.occurrence)?
                    .0;
                let first_line = file.text[..start].bytes().filter(|b| *b == b'\n').count() + 1;
                let last_line = first_line
                    + citation
                        .quote
                        .split_inclusive('\n')
                        .count()
                        .checked_sub(1)?;
                Some(CodeQuote {
                    file: path.clone(),
                    first_line,
                    last_line,
                    text: citation.quote.clone(),
                    digest: file.digest.clone(),
                })
            })
        })
        .collect()
}
