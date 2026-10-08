use super::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn summary(samples: &[Value]) -> Value {
    let mut heads = BTreeMap::<String, Option<u64>>::new();
    let mut judged_heads = BTreeSet::new();
    let mut completed = BTreeSet::new();
    let mut counters = BTreeMap::<String, u64>::new();
    let mut notice_versions = BTreeSet::new();
    let mut notices = BTreeMap::<String, u64>::new();
    let mut gaps = vec![];
    let baseline_heads = samples
        .first()
        .into_iter()
        .flat_map(|s| s["reviews"].as_array().into_iter().flatten())
        .filter(|r| r["at_ms"]["judged"].is_u64())
        .filter_map(|r| r["head"].as_str())
        .collect::<BTreeSet<_>>();
    for sample in samples {
        for notice in sample["notices"].as_array().into_iter().flatten() {
            let version = notice.to_string();
            if notice_versions.insert(version) {
                let identity = json!([notice["path"], notice["value"]]).to_string();
                *notices.entry(identity).or_default() += 1;
            }
        }
        review_metrics(
            sample,
            &baseline_heads,
            &mut heads,
            &mut judged_heads,
            &mut completed,
            &mut gaps,
        );
        for name in [
            "identical_blocked_repeats",
            "false_stall_candidates",
            "redelivery_attempts",
            "identical_redelivery_repeats",
        ] {
            let n = sample["counters"][name].as_u64().unwrap_or(0);
            counters
                .entry(name.into())
                .and_modify(|old| *old = (*old).max(n))
                .or_insert(n);
        }
        gaps.extend(sample["gaps"].as_array().into_iter().flatten().cloned());
    }
    measured(
        samples,
        heads,
        judged_heads,
        completed,
        counters,
        notices,
        gaps,
    )
}
fn review_metrics(
    sample: &Value,
    baseline_heads: &BTreeSet<&str>,
    heads: &mut BTreeMap<String, Option<u64>>,
    judged_heads: &mut BTreeSet<String>,
    completed: &mut BTreeSet<String>,
    gaps: &mut Vec<Value>,
) {
    for review in sample["reviews"].as_array().into_iter().flatten() {
        if review["at_ms"]["consumed"].is_u64() {
            completed.insert(review["id"].clone().to_string());
        }
        if review["review_class"] != "commit" {
            continue;
        }
        let Some(head) = review["head"].as_str() else {
            continue;
        };
        if baseline_heads.contains(head) {
            continue;
        }
        if review["at_ms"]["judged"].is_u64() {
            judged_heads.insert(head.into());
        }
        let latency = review["commit_ms"]
            .as_u64()
            .zip(review["at_ms"]["judged"].as_u64())
            .and_then(|(commit, verdict)| verdict.checked_sub(commit));
        heads
            .entry(head.into())
            .and_modify(|old| {
                *old = match (*old, latency) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                }
            })
            .or_insert(latency);
        if !review["missing_stages"].is_null() {
            gaps.push(json!({"review":review["id"],"missing_stages":review["missing_stages"]}));
        }
    }
}
fn measured(
    samples: &[Value],
    heads: BTreeMap<String, Option<u64>>,
    judged_heads: BTreeSet<String>,
    completed: BTreeSet<String>,
    counters: BTreeMap<String, u64>,
    notices: BTreeMap<String, u64>,
    gaps: Vec<Value>,
) -> Value {
    let first = samples.first().unwrap_or(&Value::Null);
    let last = samples.last().unwrap_or(&Value::Null);
    let elapsed = first["at_ms"]
        .as_u64()
        .zip(last["at_ms"].as_u64())
        .and_then(|(a, b)| b.checked_sub(a));
    let baseline = first["reviews"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["at_ms"]["consumed"].is_u64())
        .count();
    let rate = elapsed
        .filter(|ms| *ms > 0)
        .map(|ms| completed.len().saturating_sub(baseline) as f64 * 3_600_000.0 / ms as f64);
    let mut durations = heads.values().flatten().copied().collect::<Vec<_>>();
    durations.sort_unstable();
    json!({"acceptance_claim":false,"samples":samples.len(),"elapsed_ms":elapsed,"queue_start":first["queue_depth"],"queue_end":last["queue_depth"],
        "queue_drained":first["queue_depth"].as_u64().zip(last["queue_depth"].as_u64()).map(|(a,b)|b<=a),
        "completed_reviews_per_hour":rate,"fast_verdict_ms":{"count":durations.len(),"p50":percentile(&durations,50),"p90":percentile(&durations,90)},
        "pending_fast_verdict_heads":heads.keys().filter(|head|!judged_heads.contains(*head)).collect::<Vec<_>>(),
        "unmeasured_latency_heads":heads.iter().filter(|(_,v)|v.is_none()).map(|(k,_)|k).collect::<Vec<_>>(),
        "counters":counters,"observed_identical_notice_rewrites":notices.values().map(|n|n.saturating_sub(1)).sum::<u64>(),
        "latency_cohort":"pending at start or appeared during recording; already-judged baseline SHAs excluded",
        "notice_rewrite_count_is_a_lower_bound":true,"gaps":gaps,"measurement_targets_ms":{"p50":900_000,"p90":1_800_000},"deadlines":false})
}
fn percentile(values: &[u64], percent: usize) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values
        .get((values.len() * percent).div_ceil(100).saturating_sub(1))
        .copied()
}
