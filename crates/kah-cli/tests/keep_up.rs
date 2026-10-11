#[path = "keep_up_support/mod.rs"]
mod support;
use serde_json::json;

#[test]
fn recorder_cursor_reuses_old_metadata_and_observes_changed_and_new_records() {
    let root =
        std::env::temp_dir().join(format!("keep-up-cursor-{}", kah_cli::relay_store::nonce()));
    for name in ["requests", "results", "stage-timings", "queued"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(
        root.join("state.json"),
        json!({"pending":null,"observer":{},"worker":{}}).to_string(),
    )
    .unwrap();
    let first = root.join("requests/review-one.json");
    std::fs::write(&first,json!({"id":"review-one","purpose":"checkpoint","head":"a","trigger":{"review_class":"gate"},"unrelated":"whole source".repeat(10000)}).to_string()).unwrap();
    let mut cursor = support::Cursor::default();
    let a = support::sample_cursor(&root, &root, &mut cursor);
    let decoded = cursor.decoded;
    let b = support::sample_cursor(&root, &root, &mut cursor);
    assert_eq!(a["reviews"], b["reviews"]);
    assert_eq!(cursor.decoded, decoded, "no unchanged source decoded twice");
    std::fs::write(
        root.join("stage-timings/review-one.json"),
        json!({"at_ms":{"completed":123}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("results/review-one.json"),
        json!({"assessment":{"passed":true}}).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("requests/review-two.json"),
        json!({"id":"review-two","purpose":"checkpoint","trigger":{"review_class":"gate"}})
            .to_string(),
    )
    .unwrap();
    let c = support::sample_cursor(&root, &root, &mut cursor);
    assert_eq!(c["reviews"].as_array().unwrap().len(), 2);
    assert_eq!(c["reviews"][0]["at_ms"]["consumed"], 123);
    assert_eq!(c["reviews"][0]["assessment"]["passed"], true);
    assert!(cursor.decoded > decoded);
    let before = std::fs::read(first).unwrap();
    support::sample_cursor(&root, &root, &mut cursor);
    assert_eq!(
        std::fs::read(root.join("requests/review-one.json")).unwrap(),
        before
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn recorder_event_cursor_preserves_partial_tail_and_reads_only_appended_rows() {
    let root =
        std::env::temp_dir().join(format!("keep-up-events-{}", kah_cli::relay_store::nonce()));
    for name in ["liveness", "requests", "queued"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(
        root.join("state.json"),
        json!({"pending":null,"observer":{},"worker":{}}).to_string(),
    )
    .unwrap();
    let mut cursor = support::Cursor::default();
    let journal = root.join("liveness/events.jsonl");
    std::fs::write(&journal, "{\"state\":\"one\"}\n{\"state\":").unwrap();
    let first = support::sample_cursor(&root, &root, &mut cursor);
    assert_eq!(
        first["counters"]["liveness_events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&journal)
        .unwrap()
        .write_all(b"\"two\"}\n")
        .unwrap();
    let second = support::sample_cursor(&root, &root, &mut cursor);
    assert_eq!(
        second["counters"]["liveness_events"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        support::sample_cursor(&root, &root, &mut cursor)["counters"]["liveness_events"],
        second["counters"]["liveness_events"]
    );
    std::fs::remove_dir_all(root).unwrap();
}

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

#[test]
fn metadata_deltas_reconstruct_versions_removals_and_order_without_rewriting_old_sources() {
    let root =
        std::env::temp_dir().join(format!("keep-up-deltas-{}", kah_cli::relay_store::nonce()));
    std::fs::create_dir_all(&root).unwrap();
    let first = json!({"at_ms":1,"reviews":[{"id":"a","status":"pending"},{"id":"b","status":"done"}],"old":true});
    let next = json!({"at_ms":2,"reviews":[{"id":"b","status":"done"},{"id":"a","status":"done"}]});
    let mut recorder = support::Recording::default();
    let mut reconstructed = json!({});
    let delta = recorder.encode(&root, &first).unwrap();
    assert_eq!(
        support::reconstruct(&root, &mut reconstructed, &delta).unwrap(),
        first
    );
    let files = digest_tree(&root);
    let unchanged = recorder.encode(&root, &first).unwrap();
    assert_eq!(unchanged["changed"], json!({}));
    assert_eq!(digest_tree(&root), files);
    let delta = recorder.encode(&root, &next).unwrap();
    assert_eq!(
        support::reconstruct(&root, &mut reconstructed, &delta).unwrap(),
        next
    );
    assert_eq!(delta["removed"], json!(["old"]));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "$0 retained-sample reconstruction; requires explicit old JSONL and private output"]
fn reconstruct_old_recorder_samples_byte_for_byte() {
    use std::io::{BufRead, Write};
    let source = std::env::var("KAH_KEEP_UP_RECONSTRUCT_INPUT").unwrap();
    let output = std::path::PathBuf::from(std::env::var("KAH_KEEP_UP_RECONSTRUCT_OUTPUT").unwrap());
    std::fs::create_dir(&output).unwrap();
    let mut recorder = support::Recording::default();
    let mut reconstructed = json!({});
    let mut deltas = std::fs::File::create(output.join("samples.jsonl")).unwrap();
    let mut count = 0;
    for line in std::io::BufReader::new(std::fs::File::open(source).unwrap()).lines() {
        let line = line.unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        let delta = recorder.encode(&output, &value).unwrap();
        let restored = support::reconstruct(&output, &mut reconstructed, &delta).unwrap();
        assert_eq!(
            serde_json::to_string(&restored).unwrap().as_bytes(),
            line.as_bytes()
        );
        writeln!(deltas, "{delta}").unwrap();
        count += 1;
    }
    std::fs::write(
        output.join("reconstruction.json"),
        json!({"samples":count,"byte_identical":true,"provider_calls":0}).to_string(),
    )
    .unwrap();
    assert!(count > 0);
}
