//! Phase1: deterministic commit preparation and citation identity; no client.
//! TypeSafe: state/primitives, jev-1.13 Literal reading/Indirection/Large state,
//! citation_check and sde_cascade. Decisions use named keys, never array offsets.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
mod models;
mod ordered;
mod requests;
pub mod signals;
pub use models::*;
pub use ordered::{Prepared, Query};

pub fn digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}
pub fn key(id: &str) -> String {
    format!("item_{:x}", Sha256::digest(id.as_bytes()))
}
pub fn literal(input: &Input) -> Result<Prepared, String> {
    requests::literal(input, &selected(input))
}
pub fn prepare(input: &Input) -> Result<Plan, String> {
    let touched = selected(input);
    let literal = requests::literal(input, &touched)?;
    let (citations, absence_rows, correction_signals) = requests::citations(input, &touched)?;
    let covered: BTreeSet<_> = input.matrix.iter().map(|r| r.requirement.clone()).collect();
    let matrix_pending = touched.difference(&covered).cloned().collect();
    let matrix_prompt = format!(
        "Produce ONE matrix row per named touched requirement. Use satisfied/violated/unclear and one finding. Quote exact complete code lines with file,first_line,last_line,digest. An absence finding for a prohibition uses kind=not_violated and quotes=[]; other findings use kind=behavior. Include enough whole code context to support the finding. Never give probabilities. Unclear stays unclear; no repeated rethink of this row. Read the supplied current intent,component impact,whole diff and code;do not reconcile historical instructions or read labels. Return a JSON array of Row objects. Current revision {}. Touched IDs: {}.",
        input.revision,
        serde_json::to_string(&touched)
            .map_err(|e| format!("MEASURE_SERIALIZE stage=matrix: {e}"))?
    );
    Ok(Plan {
        revision: input.revision.clone(),
        literal,
        citations,
        touched,
        carried: input.carried.clone(),
        matrix_pending,
        absence_rows,
        correction_signals,
        matrix_prompt,
    })
}
fn selected(input: &Input) -> BTreeSet<String> {
    input
        .rules
        .iter()
        .filter(|(_, r)| input.intent.items.contains_key(&r.authority))
        .filter(|(id, rule)| {
            input.checkpoint
                || input.impacted.contains(*id)
                || input.code_changed && rule.unconditional_prohibition
        })
        .map(|(id, _)| id.clone())
        .collect()
}
pub fn verify_quote(input: &Input, quote: &CodeQuote) -> Result<(), String> {
    let file = input
        .files
        .get(&quote.file)
        .ok_or_else(|| format!("MEASURE_QUOTE_FILE stage=citation: {}", quote.file))?;
    if file.revision != input.revision
        || file.digest != quote.digest
        || file.digest != digest(file.text.as_bytes())
    {
        return Err(format!(
            "MEASURE_QUOTE_DIGEST stage=citation: {}",
            quote.file
        ));
    }
    if quote.first_line == 0 || quote.last_line < quote.first_line {
        return Err(format!(
            "MEASURE_QUOTE_RANGE stage=citation: {}",
            quote.file
        ));
    }
    let lines: Vec<_> = file.text.split_inclusive('\n').collect();
    let expected = lines
        .get(quote.first_line - 1..quote.last_line)
        .ok_or_else(|| format!("MEASURE_QUOTE_RANGE stage=citation: {}", quote.file))?
        .concat();
    if expected != quote.text || quote.text.is_empty() {
        return Err(format!("MEASURE_QUOTE_TEXT stage=citation: {}", quote.file));
    }
    Ok(())
}
