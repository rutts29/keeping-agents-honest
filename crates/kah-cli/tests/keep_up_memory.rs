#[allow(dead_code)]
#[path = "keep_up_support/mod.rs"]
mod support;
use serde_json::{Value, json};
use std::{fs, process::Command};

#[test]
fn repeated_samples_do_not_retain_repeated_review_history() {
    if std::env::var_os("KAH_KEEP_UP_MEMORY_CHILD").is_some() {
        child();
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "repeated_samples_do_not_retain_repeated_review_history",
            "--nocapture",
        ])
        .env("KAH_KEEP_UP_MEMORY_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
fn sample(at: u64) -> Value {
    let reviews = (0..500)
        .map(|n| {
            json!({"id":format!("review-{n}"),"head":format!("head-{n}"),
        "review_class":"commit","commit_ms":0,"at_ms":{"judged":60_000,"consumed":60_001},
        "missing_stages":["queued","admitted","native_done"]})
        })
        .collect::<Vec<_>>();
    json!({"at_ms":at,"queue_depth":1,"reviews":reviews,"notices":[],"counters":{},"gaps":[]})
}
fn child() {
    let mut metrics = support::Accumulator::default();
    for n in 0..200 {
        metrics.observe(&sample(n));
    }
    let before = rss();
    let mut peak = before;
    for n in 200..1000 {
        metrics.observe(&sample(n));
        if n % 100 == 0 {
            peak = peak.max(rss());
        }
    }
    let after = rss();
    peak = peak.max(after);
    println!(
        "RSS_REPEATED_SAMPLES {}",
        json!({"samples":1000,"reviews_per_sample":500,
        "warm_rss_bytes":before,"after_rss_bytes":after,"peak_rss_bytes":peak,"provider_calls":0,
        "measurement_tolerance_not_a_product_limit":16*1024*1024})
    );
    assert!(
        peak.saturating_sub(before) < 16 * 1024 * 1024,
        "RSS grew with repeated copies of unchanged history"
    );
    assert_eq!(metrics.snapshot()["samples"], 1000);
}
fn rss() -> u64 {
    // Linux Documentation/filesystems/proc.rst: VmRSS is resident memory in KiB.
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")
                .map(|v| v.split_whitespace().next().unwrap().parse::<u64>().unwrap() * 1024)
        })
        .unwrap()
}

#[test]
fn incremental_statistics_match_batch_metrics_and_count_every_gap() {
    let samples = vec![
        json!({"at_ms":0,"queue_depth":7,"reviews":[],"notices":[],"counters":{},"gaps":[]}),
        json!({"at_ms":1000,"queue_depth":4,"reviews":[
            {"id":"one","head":"a","review_class":"commit","commit_ms":0,
                "at_ms":{"judged":900,"consumed":901},"missing_stages":["native_done"]},
            {"id":"two","head":"b","review_class":"commit","commit_ms":0,"at_ms":{}}],
            "notices":[{"path":"notice","modified_ms":1,"value":{"text":"pending"}}],
            "counters":{"false_stall_candidates":1},"gaps":[{"error":"named missing receipt"}]}),
        json!({"at_ms":2000,"queue_depth":2,"reviews":[
            {"id":"one","head":"a","review_class":"commit","commit_ms":0,
                "at_ms":{"judged":900,"consumed":901},"missing_stages":["native_done"]},
            {"id":"two","head":"b","review_class":"commit","commit_ms":0,"at_ms":{"judged":1800}}],
            "notices":[{"path":"notice","modified_ms":2,"value":{"text":"pending"}}],
            "counters":{"false_stall_candidates":1},"gaps":[{"error":"named missing receipt"}]}),
    ];
    let expected = support::summary(&samples);
    let mut metrics = support::Accumulator::default();
    let root = std::env::temp_dir().join(format!("keep-up-memory-cas-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut recording = support::Recording::default();
    let mut rebuilt = json!({});
    for sample in &samples {
        metrics.observe(sample);
        let delta = recording.encode(&root, sample).unwrap();
        let expanded = support::reconstruct(&root, &mut rebuilt, &delta).unwrap();
        assert_eq!(
            serde_json::to_vec(&expanded).unwrap(),
            serde_json::to_vec(sample).unwrap()
        );
    }
    let actual = metrics.snapshot();
    for (key, value) in expected.as_object().unwrap() {
        if key != "gaps" {
            assert_eq!(&actual[key], value, "metric {key}");
        }
    }
    let mut gaps = std::collections::BTreeMap::<String, u64>::new();
    for gap in expected["gaps"].as_array().unwrap() {
        *gaps.entry(gap.to_string()).or_default() += 1;
    }
    let observed = actual["gap_occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| (entry["gap"].to_string(), entry["count"].as_u64().unwrap()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(observed, gaps);
    assert_eq!(actual["observed_identical_notice_rewrites"], 1);
    fs::remove_dir_all(root).unwrap();
}
