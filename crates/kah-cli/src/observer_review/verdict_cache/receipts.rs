//! Latest native execution identity; never guess tests from command wording.
use crate::{Result, observer_review::state::Request};
use kah_observer::{
    decision_units::Requirement,
    review_evidence::{Citation, Document},
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
pub fn latest(request: &Request, r: &Requirement) -> Result<BTreeMap<String, Value>> {
    r.checks.iter().map(|check| {
        let command = r.citations.get(check.command_citation).ok_or("REVIEW_CACHE_RECEIPT stage=verdict_cache: command citation missing")?;
        let result = r.citations.get(check.result_citation).ok_or("REVIEW_CACHE_RECEIPT stage=verdict_cache: result citation missing")?;
        if command.source != result.source { return Err("REVIEW_CACHE_RECEIPT stage=verdict_cache: split execution source".into()); }
        let doc = request.documents.iter().find(|d| d.id == command.source && d.kind == "worker_transcript")
            .ok_or("REVIEW_CACHE_RECEIPT stage=verdict_cache: native execution source missing")?;
        let commands = links(doc,command)?;
        let results = links(doc,result)?;
        let matching:Vec<_> = commands.iter().filter(|c|results.iter().any(|v|v["id"]==c["id"] && v["result"]==true)).collect();
        let ids:BTreeSet<_> = matching.iter().filter_map(|v|v["id"].as_str()).collect();
        if ids.len()!=1 { return Err("REVIEW_CACHE_RECEIPT stage=verdict_cache: ambiguous or unbound execution".into()); }
        let bound = matching[0];
        let bound_result = results.iter().find(|v|v["id"]==bound["id"] && v["result"]==true)
            .ok_or("REVIEW_CACHE_RECEIPT stage=verdict_cache: bound result missing")?;
        if bound["command"].as_str().is_none() || bound["cwd"].as_str().is_none() {
            return Err("REVIEW_CACHE_RECEIPT stage=verdict_cache: native command/cwd unavailable".into());
        }
        let latest = doc.native_records.iter().rev().flat_map(|record| record.context["executions"].as_array().into_iter().flatten().rev())
            .find(|value|value["command"]==bound["command"] && value["cwd"]==bound["cwd"])
            .ok_or("REVIEW_CACHE_RECEIPT stage=verdict_cache: latest execution missing")?;
        if latest["id"] != bound["id"] || latest["result"] != true || latest["result_digest"] != bound_result["result_digest"] {
            return Err(format!("REVIEW_CACHE_RECEIPT stage=verdict_cache: check {:?} cites an older execution; latest native result requires fresh judgment",check.name).into());
        }
        Ok((check.name.clone(),json!({"source":doc.uri,"id":latest["id"],"command":latest["command"],"cwd":latest["cwd"],
            "result":latest["result"],"result_digest":latest["result_digest"]})))
    }).collect()
}
fn links<'a>(doc: &'a Document, citation: &Citation) -> Result<Vec<&'a Value>> {
    let start = doc
        .text
        .match_indices(&citation.quote)
        .nth(citation.occurrence)
        .map(|(start, _)| start)
        .ok_or("REVIEW_CACHE_RECEIPT stage=verdict_cache: cited unit missing")?;
    let end = start + citation.quote.len();
    Ok(doc
        .native_records
        .iter()
        .filter(|record| record.start_byte < end && record.end_byte > start)
        .flat_map(|record| {
            record.context["executions"]
                .as_array()
                .into_iter()
                .flatten()
        })
        .collect())
}
