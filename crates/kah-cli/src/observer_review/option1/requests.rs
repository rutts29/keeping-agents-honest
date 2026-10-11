//! Identical measurement wording/packing, now used before native admission.
use crate::{
    Result,
    commit_measure::{decisive_bookkeeping, decisive_packing, estimate},
};
use kah_observer::{
    commit_check,
    context_budget::Calibration,
    judge::batch::{Tokens, WireRequest},
};
use serde_json::Value;
use std::collections::BTreeMap;
pub fn build(
    input: &commit_check::Input,
    calibration: Option<&Calibration>,
) -> Result<(Vec<WireRequest>, Value)> {
    let (short, map) = decisive_bookkeeping::literal(&commit_check::literal(input)?)?;
    let short = commit_check::rule_in_question(&short)?;
    let packed = if let Some(c) = calibration {
        let ratio = c.input_tokens as f64 / c.input_utf8_bytes as f64;
        let estimate = estimate::Calibration {
            samples: 1,
            input_tokens: c.input_tokens,
            input_utf8_bytes: c.input_utf8_bytes,
            minimum_ratio: ratio,
            maximum_ratio: ratio,
            source_digests: BTreeMap::new(),
        };
        decisive_packing::pack(&short, &estimate)?
    } else {
        vec![short]
    };
    let requests = packed
        .into_iter()
        .filter(|p| !p.questions.is_empty())
        .map(|p| {
            let body = p.body()?;
            Ok(WireRequest {
                id: format!(
                    "commit-{}-{}",
                    input.revision,
                    kah_observer::context_selection::digest(&serde_json::from_str::<Value>(&body)?)
                ),
                tokens: Tokens::estimate(&body, calibration)?,
                body,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((requests, map))
}
