//! Offline gold-context preparation and opt-in live Jev replay. Never changes policy.
use crate::judge::{self, Judgment};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub mod dataset;
mod scoring;
mod storage;
use dataset::{Case, Row};
use storage::Output;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Record {
    pub id: String,
    pub request_digest: String,
    pub source: String,
    pub judgment: Option<Judgment>,
    pub error: Option<String>,
}

pub fn run(home: &Path, input: &Path, validation: bool) -> Result<PathBuf, String> {
    let dataset = input.canonicalize().map_err(storage::io_error)?;
    let bytes = fs::read(&dataset).map_err(storage::io_error)?;
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("EVAL_DATASET_BOUND stage=replay".into());
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "EVAL_DATASET_UTF8 stage=replay")?;
    let rows = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str::<Row>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "EVAL_DATASET_JSON stage=replay")?;
    let cases = dataset::prepare(rows, validation)?;
    let digest = hex(&bytes);
    let output = Output::open(&dataset, &digest, validation)?;
    output.write("manifest.json", &json!({"datasetSha256":digest,"questionSetVersion":judge::questions::VERSION,"questionSourceSha256":hex(include_bytes!("../judge/questions.rs")),"validation":validation,"rows":cases.len(),"modelAlias":"jev-latest","thresholds":{"steering":judge::questions::STEERING_PROBABILITY,"kind":judge::questions::TYPE_CONFIDENCE,"replacementProbability":judge::questions::REPLACE_PROBABILITY,"replacementConfidence":judge::questions::REPLACE_CONFIDENCE},"thresholdsMayChangeAutomatically":false}))?;
    let cached = output.cache()?;
    let mut scores = scoring::Scores::default();
    let mut disagreements = vec![];
    for (index, case) in cases.iter().enumerate() {
        let request_digest = hex(json!({"state":case.state,"questions":case.questions})
            .to_string()
            .as_bytes());
        let record = match cached.get(&case.row.id) {
            Some(record) if record.request_digest == request_digest => record.clone(),
            Some(_) => return Err("EVAL_CACHE_STALE stage=replay".into()),
            None => {
                let record = evaluate(home, case, request_digest);
                output.append(&record)?;
                record
            }
        };
        if !scores.add(case, &record) {
            disagreements.push(json!({"id":case.row.id,"session":case.row.session,"ts":case.row.ts,"gold":case.row.gold,"result":record}));
        }
        if (index + 1) % 25 == 0 {
            eprintln!(
                "kah replay: {}/{} rows evaluated; policy unchanged",
                index + 1,
                cases.len()
            );
        }
    }
    let report = json!({"datasetSha256":digest,"questionSetVersion":judge::questions::VERSION,"split":if validation {"validation"} else {"development"},"splitRule":"IDs ending in 0, 1 or 2 are held out in both sessions; gold prior context remains teacher-forced.","thresholdsChanged":false,"cachedRows":cases.iter().filter(|c|cached.contains_key(&c.row.id)).count(),"report":scores.report(),"limitations":"One labeler; disagreements require review. Context is rebuilt from gold, not predictions, and includes the provided bounded previous assistant excerpt as data. Metrics are not free-running task success."});
    output.write("report.json", &report)?;
    output.write("disagreements.json", &json!(disagreements))?;
    Ok(output.directory.join("report.json"))
}
fn evaluate(home: &Path, case: &Case, request_digest: String) -> Record {
    let role = kah_codex::authorship::classify(&case.row.user_text, false);
    let source = match role {
        kah_core::intent_history::SourceRole::User => "user",
        kah_core::intent_history::SourceRole::Delegate => "delegate",
        _ => "system",
    };
    let mut record = Record {
        id: case.row.id.clone(),
        request_digest,
        source: source.into(),
        judgment: None,
        error: None,
    };
    if source != "user" {
        return record;
    }
    match judge::routing::evaluate(
        home,
        judge::routing::Backend::Jev,
        "jev-latest",
        &case.state,
        &case.questions,
    ) {
        Ok(judgment) => record.judgment = Some(judgment),
        Err(error) => record.error = Some(error.to_string()),
    }
    record
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
