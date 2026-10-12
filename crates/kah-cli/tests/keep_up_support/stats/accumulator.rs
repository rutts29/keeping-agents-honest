//! Retain metrics by evidence identity, never copies of history per sample.
//! Every complete sample and its chronology still lives in the lossless CAS log.
use super::*;
use sha2::{Digest, Sha256};

#[derive(Default)]
pub struct Accumulator {
    first: Option<Value>,
    last: Value,
    samples: u64,
    baseline_heads: BTreeSet<String>,
    heads: BTreeMap<String, Option<u64>>,
    judged_heads: BTreeSet<String>,
    completed: BTreeSet<String>,
    counters: BTreeMap<String, u64>,
    notice_versions: BTreeSet<String>,
    notices: BTreeMap<String, u64>,
    gaps: BTreeMap<String, (Value, u64)>,
}
impl Accumulator {
    pub fn observe(&mut self, sample: &Value) {
        if self.first.is_none() {
            self.baseline_heads = sample["reviews"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["at_ms"]["judged"].is_u64())
                .filter_map(|r| r["head"].as_str().map(str::to_owned))
                .collect();
            self.first = Some(sample.clone());
        }
        self.samples += 1;
        self.last = sample.clone();
        self.observe_notices(sample);
        let baseline = self.baseline_heads.iter().map(String::as_str).collect();
        let mut gaps = vec![];
        review_metrics(
            sample,
            &baseline,
            &mut self.heads,
            &mut self.judged_heads,
            &mut self.completed,
            &mut gaps,
        );
        gaps.extend(sample["gaps"].as_array().into_iter().flatten().cloned());
        for gap in gaps {
            self.gaps
                .entry(identity(&gap))
                .and_modify(|(_, count)| *count += 1)
                .or_insert((gap, 1));
        }
        self.observe_counters(sample);
    }
    fn observe_notices(&mut self, sample: &Value) {
        for notice in sample["notices"].as_array().into_iter().flatten() {
            if self.notice_versions.insert(identity(notice)) {
                let key = identity(&json!([notice["path"], notice["value"]]));
                *self.notices.entry(key).or_default() += 1;
            }
        }
    }
    fn observe_counters(&mut self, sample: &Value) {
        for name in [
            "identical_blocked_repeats",
            "false_stall_candidates",
            "redelivery_attempts",
            "identical_redelivery_repeats",
        ] {
            let n = sample["counters"][name].as_u64().unwrap_or(0);
            self.counters
                .entry(name.into())
                .and_modify(|old| *old = (*old).max(n))
                .or_insert(n);
        }
    }
    pub fn snapshot(&self) -> Value {
        let boundaries = match &self.first {
            Some(first) => vec![first.clone(), self.last.clone()],
            None => vec![],
        };
        let gaps = self.gaps.values().map(|(gap, _)| gap.clone()).collect();
        let mut result = measured(
            &boundaries,
            self.heads.clone(),
            self.judged_heads.clone(),
            self.completed.clone(),
            self.counters.clone(),
            self.notices.clone(),
            gaps,
        );
        result["samples"] = self.samples.into();
        result["gap_occurrences"] = self
            .gaps
            .values()
            .map(|(gap, count)| json!({"gap":gap,"count":count}))
            .collect();
        result["gap_history"] = json!({"source":"samples.jsonl", "format":"kah-keep-up-delta-v1",
            "lossless":true,"note":"Distinct gaps and counts summarized here; full per-sample chronology retained in CAS-backed samples."});
        result
    }
}
fn identity(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
