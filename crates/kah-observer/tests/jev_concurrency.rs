//! $0 retained-data HTTP integration; never initializes an authenticated client.
use kah_observer::judge::{
    batch::{Batch, Mode},
    jev::HttpClient,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
mod jev_concurrency_support;
use jev_concurrency_support::{Corpus, Stub};

#[test]
#[ignore = "293 retained Phase1 wire requests; explicit private fixture root, no live Jev"]
fn retained_phase1_requests_keep_identity_and_run_faster_with_paced_concurrency() {
    let corpus = Corpus::load();
    let root = private_root("identity");
    let stub = Stub::start(&corpus, false);
    let client = HttpClient::replay(stub.address).unwrap();
    let sequential = Batch::new(&root.join("sequential"), &root.join("sequential-rate"))
        .unwrap()
        .run(&corpus.requests, &client, Mode::Sequential)
        .unwrap();
    let concurrent = Batch::new(&root.join("concurrent"), &root.join("concurrent-rate"))
        .unwrap()
        .run(&corpus.requests, &client, Mode::Concurrent)
        .unwrap();
    let sequential_bytes = sequential.evidence_bytes().unwrap();
    let concurrent_bytes = concurrent.evidence_bytes().unwrap();
    fs::write(root.join("sequential-evidence.json"), &sequential_bytes).unwrap();
    fs::write(root.join("concurrent-evidence.json"), &concurrent_bytes).unwrap();
    assert_eq!(sequential_bytes, concurrent_bytes);
    let identity_sha256 = format!("{:x}", Sha256::digest(&concurrent_bytes));
    assert!(concurrent.telemetry.wall_ms < sequential.telemetry.wall_ms);
    assert!(concurrent.telemetry.peak_in_flight > 1);
    corpus.assert_unchanged();
    evidence(
        &root,
        "identity",
        json!({"sequential":sequential.telemetry,"concurrent":concurrent.telemetry,"identity_sha256":identity_sha256,
        "identity_scope":["wire requests","raw answers","evidence payloads","verdicts","fixed order"],
        "separate_telemetry":["timing","headers","capture paths"]}),
    );
}
#[test]
#[ignore = "retained Phase1 HTTP replay with lower provider rate and retry headers; no live Jev"]
fn lower_stub_limits_and_retry_after_finish_without_judgment_drift() {
    let corpus = Corpus::load();
    let root = private_root("lower-rate");
    let stub = Stub::start(&corpus, true);
    let client = HttpClient::replay(stub.address).unwrap();
    let result = Batch::new(&root.join("batch"), &root.join("rate"))
        .unwrap()
        .run(&corpus.requests, &client, Mode::Concurrent)
        .unwrap();
    corpus.assert_answers(&result);
    assert!(result.telemetry.http_429 > 0 && result.telemetry.http_529 > 0);
    assert_eq!(
        result.telemetry.tokens_per_second_label,
        "exact plus estimated, unconfirmed"
    );
    for line in fs::read_to_string(root.join("batch/events.jsonl"))
        .unwrap()
        .lines()
    {
        let event: Value = serde_json::from_str(line).unwrap();
        if event["kind"] == "backoff" {
            let request = corpus
                .requests
                .iter()
                .find(|r| event["request"] == r.id)
                .unwrap();
            assert_eq!(event["wire_request"], request.body);
            assert_eq!(event["raw_response"], "{\"error\":\"rate limit\"}");
        }
    }
    stub.assert_retry_after_respected();
    corpus.assert_unchanged();
    evidence(&root, "lower-rate", json!(result.telemetry));
}
fn private_root(label: &str) -> PathBuf {
    let base = PathBuf::from(std::env::var_os("KAH_CONCURRENCY_FIXTURE").unwrap());
    let fixture_base = PathBuf::from(
        std::env::var_os("KAH_TEST_FIXTURE_BASE").expect("explicit fixture base required"),
    );
    assert!(fixture_base.is_absolute());
    assert!(base.is_absolute() && base.starts_with(&fixture_base));
    let root = base.join(label);
    kah_core::run_store::ensure_private_directory(&root).unwrap();
    root
}
fn evidence(root: &std::path::Path, name: &str, details: Value) {
    let value = json!({"case":name,"details":details,"real_jev_calls":0,"native_calls":0,"final_acceptance":false});
    fs::write(
        root.join("evidence.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "actual process kill/restart against retained local HTTP replay; no live Jev"]
fn killed_batch_reuses_only_durable_answers_and_sends_unanswered_requests() {
    let corpus = Corpus::load();
    let root = private_root("restart");
    if let Some(address) = std::env::var_os("KAH_CONCURRENCY_CHILD") {
        let client = HttpClient::replay(address.to_str().unwrap().parse().unwrap()).unwrap();
        let outcome = Batch::new(&root.join("batch"), &root.join("rate"))
            .unwrap()
            .run(&corpus.requests, &client, Mode::Concurrent)
            .unwrap();
        corpus.assert_answers(&outcome);
        fs::write(
            root.join("final-evidence.json"),
            outcome.evidence_bytes().unwrap(),
        )
        .unwrap();
        return;
    }
    let stub = Stub::start(&corpus, false);
    let mut first = child(stub.address);
    let receipt_root = root.join("batch/receipts");
    loop {
        let complete = receipts(&receipt_root);
        if complete.len() >= 10 {
            break;
        }
        assert!(
            first.try_wait().unwrap().is_none(),
            "child ended before the kill boundary"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    first.kill().unwrap();
    let status = first.wait().unwrap();
    assert!(!status.success());
    let answered = receipts(&receipt_root);
    assert!(answered.len() < corpus.requests.len());
    let before = stub.counts();
    assert!(child(stub.address).wait().unwrap().success());
    let after = stub.counts();
    for receipt in &answered {
        assert_eq!(
            before[&receipt.request.id], after[&receipt.request.id],
            "answered request resent"
        );
    }
    assert_eq!(receipts(&receipt_root).len(), corpus.requests.len());
    corpus.assert_unchanged();
    evidence(
        &root,
        "restart",
        json!({"durable_before":answered.len(),"completed_after":corpus.requests.len(),"answered_resent":0,"killed_process":true}),
    );
}
fn child(address: std::net::SocketAddr) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "killed_batch_reuses_only_durable_answers_and_sends_unanswered_requests",
            "--ignored",
            "--nocapture",
        ])
        .env("KAH_CONCURRENCY_CHILD", address.to_string())
        .spawn()
        .unwrap()
}
fn receipts(root: &std::path::Path) -> Vec<kah_observer::judge::batch::Receipt> {
    let Ok(entries) = fs::read_dir(root) else {
        return vec![];
    };
    entries
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .map(|p| serde_json::from_slice(&fs::read(p).unwrap()).unwrap())
        .collect()
}

#[test]
#[ignore = "unknown/oversized estimates on intact retained bodies; local HTTP replay only"]
fn token_estimates_only_change_start_time_and_exact_usage_corrects_the_window() {
    let corpus = Corpus::load();
    let root = private_root("estimates");
    let stub = Stub::start(&corpus, false);
    let client = HttpClient::replay(stub.address).unwrap();
    let mut requests = corpus.requests[..2].to_vec();
    for request in &mut requests {
        request.tokens = kah_observer::judge::batch::Tokens::estimate(&request.body, None).unwrap();
        assert_eq!(request.tokens.estimated, 64_000.0);
        assert_eq!(request.tokens.source, "documented_64K_reservation");
    }
    let large = corpus
        .requests
        .iter()
        .find(|r| r.tokens.recorded_exact.is_none() && r.tokens.estimated > 100_000.0)
        .unwrap()
        .clone();
    requests.push(large.clone());
    let result = Batch::new(&root.join("batch"), &root.join("rate"))
        .unwrap()
        .run(&requests, &client, Mode::Concurrent)
        .unwrap();
    let events = fs::read_to_string(root.join("batch/events.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let start = |id: &str| {
        events
            .iter()
            .find(|e| e["kind"] == "start" && e["request"] == id)
            .unwrap()["at_ms"]
            .as_u64()
            .unwrap()
    };
    assert!(
        start(&requests[0].id).abs_diff(start(&requests[1].id)) < 1000,
        "exact response must release the 64K reservation"
    );
    let rejection = events
        .iter()
        .find(|e| e["kind"] == "finish" && e["request"] == large.id)
        .unwrap();
    assert_eq!(rejection["count_label"], "estimated, unconfirmed");
    assert!(rejection["exact_input_tokens"].is_null());
    for (before, after) in requests.iter().zip(&result.responses) {
        assert_eq!(before.body, after.request.body);
    }
    corpus.assert_unchanged();
    evidence(
        &root,
        "estimates",
        json!({"unknown_reservation":64000,"large_estimate":large.tokens.estimated,
        "body_unchanged":true,"exact_replacement":true,"rejected_count":"estimated, unconfirmed"}),
    );
}
