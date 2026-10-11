//! Recorded bodies/answers through the production screen and track2 HTTP stub.
//! No credentials, native agent, provider requests, scoring-label construction.
#[allow(dead_code)]
#[path = "../../kah-observer/tests/jev_concurrency_support/stub.rs"]
mod transport;
use kah_observer::{
    commit_review::{DoubtLine, transport::evaluate},
    judge::{
        batch::{Batch, Tokens, WireRequest},
        jev::HttpClient,
    },
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[test]
#[ignore = "retained 24+7 replay; run via make gate PHASE=2-step5-replay"]
fn retained_cases_use_the_production_literal_screen_with_identical_wakes() {
    let output = PathBuf::from(std::env::var_os("KAH_OPTION1_REPLAY_OUTPUT").unwrap());
    kah_core::run_store::ensure_private_directory(&output).unwrap();
    let policy: DoubtLine =
        serde_json::from_str(include_str!("../../../assets/observer/commit-policy.json")).unwrap();
    let corpora = [
        Path::new(
            "/mnt/kah-fixtures/cargo/synthetic-preparation/jev/complete.json",
        ),
        Path::new(
            "/mnt/kah-fixtures/cargo/synthetic-validation/jev/complete.json",
        ),
    ];
    let mut groups = BTreeMap::<String, Vec<Value>>::new();
    for path in corpora {
        let archive: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        for row in archive["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["job"]["stage"] == "literal")
        {
            groups
                .entry(row["job"]["case_id"].as_str().unwrap().into())
                .or_default()
                .push(row.clone());
        }
    }
    assert_eq!(groups.len(), 31);
    let mut results = vec![];
    for (case, rows) in groups {
        results.push(replay(&output, &case, &rows, &policy));
    }
    fs::write(output.join("production-replay.json"),serde_json::to_vec_pretty(&json!({"cases":results,"real_jev_calls":0,"native_calls":0,"source":"unchanged recorded bodies; production concurrent screen; credential-free HTTP stub"})).unwrap()).unwrap();
}
fn replay(output: &Path, case: &str, rows: &[Value], policy: &DoubtLine) -> Value {
    let (corpus, expected, rejected) = load(rows);
    let stub = transport::Stub::start(&corpus, false);
    let client = HttpClient::replay(stub.address).unwrap();
    let batch = Batch::new(&output.join(case).join("responses"), &output.join("rate")).unwrap();
    let record = evaluate(
        "frozen-worker",
        case,
        policy,
        &batch,
        &corpus.requests,
        &client,
    )
    .unwrap();
    assert_eq!(record.woke, expected.iter().any(|n| *n >= policy.line));
    assert_eq!(record.gaps.len(), rejected);
    assert_eq!(record.answers.len(), expected.len());
    let count = stub.counts();
    let repeated = evaluate(
        "frozen-worker",
        case,
        policy,
        &batch,
        &corpus.requests,
        &client,
    )
    .unwrap();
    assert_eq!(
        stub.counts(),
        count,
        "cached oversized bodies must never be resent"
    );
    assert_eq!(repeated.woke, record.woke);
    json!({"case":case,"record":record,"same_answers":true,"same_wake":true,"cached_rejection_not_resent":true,"requests":corpus.requests.len()})
}

struct Corpus {
    requests: Vec<WireRequest>,
    replies: BTreeMap<String, (u16, String, u64, u64)>,
}

fn load(rows: &[Value]) -> (Corpus, Vec<f64>, usize) {
    let mut requests = vec![];
    let mut replies = BTreeMap::new();
    let mut expected = Vec::<f64>::new();
    let mut rejected = 0;
    for (index, row) in rows.iter().enumerate() {
        let body = fs::read_to_string(row["job"]["path"].as_str().unwrap()).unwrap();
        assert_eq!(
            kah_observer::commit_check::digest(body.as_bytes()),
            row["job"]["digest"]
        );
        let id = format!("phase1-{index}");
        let tokens = Tokens::estimate(&body, None).unwrap();
        let status = row["error_response"]["status"].as_u64().unwrap_or(200) as u16;
        let response = if status == 200 {
            &row["response"]
        } else {
            rejected += 1;
            &row["error_response"]["body"]
        };
        if status == 200 {
            expected.extend(
                response["answers"]
                    .as_object()
                    .unwrap()
                    .values()
                    .map(|a| a["noul"].as_f64().unwrap()),
            );
        }
        replies.insert(
            id.clone(),
            (
                status,
                serde_json::to_string(response).unwrap(),
                response["usage"]["input_tokens"].as_u64().unwrap_or(0),
                0,
            ),
        );
        requests.push(WireRequest { id, body, tokens });
    }
    (Corpus { requests, replies }, expected, rejected)
}
