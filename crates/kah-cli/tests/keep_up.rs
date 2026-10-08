#[path = "keep_up_support/mod.rs"]
mod support;
use serde_json::json;

#[test]
fn metrics_preserve_missing_stages_pending_heads_and_repeated_events() {
    let samples = vec![
        json!({"at_ms":0,"queue_depth":4,"reviews":[],"counters":{}}),
        json!({"at_ms":3_600_000,"queue_depth":2,"reviews":[
        {"id":"one","head":"a","review_class":"commit","commit_ms":0,"at_ms":{"judged":900_000,"consumed":900_001}},
        {"id":"two","head":"b","review_class":"commit","commit_ms":0,"at_ms":{"judged":1_800_000}},
        {"id":"three","head":"c","review_class":"commit","commit_ms":0,"at_ms":{}},
        {"id":"four","head":"d","review_class":"commit","commit_ms":null,"at_ms":{"judged":900_000}}],
        "counters":{"identical_blocked_repeats":2,"false_stall_candidates":1,"redelivery_attempts":3}}),
    ];
    let summary = support::summary(&samples);
    assert_eq!(summary["queue_drained"], true);
    assert_eq!(summary["fast_verdict_ms"]["p50"], 900_000);
    assert_eq!(summary["fast_verdict_ms"]["p90"], 1_800_000);
    assert_eq!(summary["pending_fast_verdict_heads"], json!(["c"]));
    assert_eq!(summary["unmeasured_latency_heads"], json!(["c", "d"]));
    assert_eq!(summary["completed_reviews_per_hour"], 1.0);
    assert_eq!(summary["counters"]["identical_blocked_repeats"], 2);
}

#[test]
#[ignore = "Read-only KEEP-UP recorder: requires explicit live review root, repository and volume output; run via make gate PHASE=5o-keep-up"]
fn record_builder_keep_up() {
    support::run().unwrap();
}

#[test]
fn snapshot_counts_real_repeated_notices_and_leaves_every_source_byte_unchanged() {
    let root = std::env::temp_dir().join(format!(
        "kah-keep-up-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let review = root.join("review");
    for dir in ["requests", "queued", "stage-timings", "liveness"] {
        std::fs::create_dir_all(review.join(dir)).unwrap();
    }
    std::fs::create_dir_all(root.join("outbox")).unwrap();
    std::fs::write(root.join("outbox/submitted-review-one.json"), "{}").unwrap();
    std::fs::write(review.join("state.json"),json!({"pending":{"id":"review-one"},"observer":{"workspace":root},"worker":{"session":"fixture"}}).to_string()).unwrap();
    std::fs::write(review.join("requests/review-one.json"),json!({"id":"review-one","head":"a","purpose":"checkpoint","trigger":{"review_class":"commit"}}).to_string()).unwrap();
    std::fs::write(review.join("queued/one.json"), "{}").unwrap();
    let at = kah_observer::session::now_ms() + 100;
    let blocked = json!({"state":"validation_blocked","progress":{"sample":{"request":"review-one","now_ms":at,"cause":"same error"}}});
    let stall = json!({"state":"stalled","progress":{"sample":{"request":"review-one","now_ms":at,"cause":"after ACK"}}});
    std::fs::write(
        review.join("liveness/events.jsonl"),
        format!("{blocked}\n{blocked}\n{blocked}\n{stall}\n"),
    )
    .unwrap();
    let before = digest_tree(&root);
    let sample = support::sample(&review, &root);
    assert_eq!(sample["queue_depth"], 2);
    assert_eq!(sample["counters"]["identical_blocked_repeats"], 2);
    assert_eq!(sample["counters"]["false_stall_candidates"], 1);
    assert!(
        sample["reviews"][0]["missing_stages"]
            .as_array()
            .unwrap()
            .contains(&json!("admitted"))
    );
    assert_eq!(digest_tree(&root), before);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn durable_admission_and_native_done_are_preserved_while_evidence_ack_is_separate() {
    let root =
        std::env::temp_dir().join(format!("keep-up-stages-{}", kah_cli::relay_store::nonce()));
    let review = root.join("review");
    for dir in ["requests", "queued", "stage-timings"] {
        std::fs::create_dir_all(review.join(dir)).unwrap();
    }
    std::fs::create_dir_all(root.join("outbox")).unwrap();
    std::fs::write(
        review.join("state.json"),
        json!({"pending":null,"observer":{"workspace":root},"worker":{"session":"fixture"}})
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        review.join("requests/review-one.json"),
        json!({"id":"review-one","purpose":"checkpoint","trigger":{"review_class":"commit"}})
            .to_string(),
    )
    .unwrap();
    std::fs::write(
        review.join("stage-timings/review-one.json"),
        json!({"at_ms":{"admitted":123,"native_done":456,"completed":789}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        review.join("last-admitted-lane.json"),
        json!({"request":"another","at_ms":999}).to_string(),
    )
    .unwrap();
    std::fs::write(root.join("outbox/submitted-review-one.json"), "{}").unwrap();
    let result = support::sample(&review, &root);
    let row = &result["reviews"][0];
    assert_eq!(row["at_ms"]["admitted"], 123);
    assert_eq!(row["at_ms"]["native_done"], 456);
    assert!(row["at_ms"]["evidence_ack"].is_u64());
    assert_eq!(
        row["stage_sources"]["native_done"],
        "per-request validated native Stop host receipt"
    );
    std::fs::remove_dir_all(root).unwrap();
}
fn digest_tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut files = vec![];
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(digest_tree(&path));
        } else {
            files.push((path.clone(), std::fs::read(path).unwrap()));
        }
    }
    files.sort();
    files
}

#[test]
fn latency_cohort_excludes_old_verdicts_and_notice_sampling_is_not_redelivery() {
    let notice = json!({"path":"validation-blocked.json","modified_ms":1,"value":{"request":"same","text":"blocked"}});
    let mut rewritten = notice.clone();
    rewritten["modified_ms"] = 2.into();
    let samples = vec![
        json!({"at_ms":4_000_000,"queue_depth":2,"notices":[notice],"reviews":[{"id":"old","head":"old","review_class":"commit","commit_ms":0,"at_ms":{"judged":1_000_000,"consumed":1_000_001}}]}),
        json!({"at_ms":5_000_000,"queue_depth":2,"notices":[notice],"reviews":[]}),
        json!({"at_ms":8_000_000,"queue_depth":0,"notices":[rewritten],"reviews":[{"id":"new","head":"new","review_class":"commit","commit_ms":4_000_000,"at_ms":{"judged":8_000_000}}]}),
    ];
    let result = support::summary(&samples);
    assert_eq!(result["fast_verdict_ms"]["count"], 1);
    assert_eq!(result["fast_verdict_ms"]["p90"], 4_000_000);
    assert_eq!(result["observed_identical_notice_rewrites"], 1);
    assert_eq!(result["deadlines"], false);
}
