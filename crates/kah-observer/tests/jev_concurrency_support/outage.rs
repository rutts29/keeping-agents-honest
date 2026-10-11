//! One retained three-request batch; HTTP service failure is not an answered judgment.
use super::*;
use crate::{evidence, private_root, receipts};
use kah_observer::judge::{
    availability,
    batch::{Batch, Mode},
    jev::HttpClient,
};
use serde_json::json;
#[test]
#[ignore = "explicit retained-data loopback outage replay; zero live Jev"]
fn failed_review_keeps_answers_and_resumes_only_unanswered_after_backoff() {
    let status: u16 = std::env::var("KAH_OUTAGE_STATUS")
        .unwrap_or_else(|_| "503".into())
        .parse()
        .unwrap();
    let corpus = Corpus::load();
    let root = private_root(&format!("outage-{status}"));
    let stub = Stub::outage(&corpus, status);
    let client = HttpClient::replay(stub.address).unwrap();
    let batch = Batch::new(&root.join("batch"), &root.join("rate")).unwrap();
    let error = batch
        .run(&corpus.requests[..3], &client, Mode::Concurrent)
        .err()
        .expect("service error must not become an answer");
    assert_eq!(error.status, Some(status));
    assert!(
        availability::outage(&error.to_string()),
        "service error must retain pending review"
    );
    let answers = receipts(&root.join("batch/receipts"));
    assert_eq!(answers.len(), 2);
    assert!(answers.iter().all(|r| r.request.id != "phase1-1"));
    let before = stub.counts();
    recover_after_backoff(&root.join("home"), &error);
    let result = batch
        .run(&corpus.requests[..3], &client, Mode::Concurrent)
        .unwrap();
    let after = stub.counts();
    for saved in &answers {
        assert_eq!(before[&saved.request.id], after[&saved.request.id]);
        let resumed = result
            .responses
            .iter()
            .find(|r| r.request.id == saved.request.id)
            .unwrap();
        assert!(resumed.reused);
        assert_eq!(resumed.response.body, saved.response.body);
    }
    assert_eq!(after["phase1-1"], before["phase1-1"] + 1);
    for response in &result.responses {
        let (status, body, _, _) = &corpus.replies[&response.request.id];
        assert_eq!(&response.response.status, status);
        assert_eq!(&response.response.body, body);
    }
    corpus.assert_unchanged();
    evidence(
        &root,
        "outage-rerun",
        json!({"http_status":status,"returned_error":error.to_string(),
        "durable_before":answers.len(),"completed_after":result.responses.len(),"answered_resent":0,
        "only_unanswered_sent":true,"backoff_suppressed_probe":true,"backoff_ms":15000}),
    );
}
fn recover_after_backoff(home: &std::path::Path, error: &kah_observer::judge::Error) {
    availability::record_error(home, &error.to_string()).unwrap();
    let epoch = availability::snapshot(home).unwrap().unwrap()["observed_ms"]
        .as_u64()
        .unwrap();
    availability::recovery::with_probe(home, epoch + 14_999, || panic!("probe before backoff"))
        .unwrap();
    assert!(availability::unavailable(home).unwrap());
    availability::recovery::with_probe(home, epoch + 15_000, || Ok(())).unwrap();
    assert!(!availability::unavailable(home).unwrap());
}
