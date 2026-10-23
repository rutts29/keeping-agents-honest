//! Authored literal bodies and answers through the production screen and a
//! local HTTP stub. No credentials, native agent or provider requests.
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
use std::{collections::BTreeMap, path::Path};

struct Corpus {
    requests: Vec<WireRequest>,
    replies: BTreeMap<String, (u16, String, u64, u64)>,
}

/// A literal request body: one changed hunk and one rule question.
fn body(rule: &str, hunk: &str) -> String {
    json!({"model":"jev-1.13.0",
        "state":{"diff_hunks":{"h1":{"file":"src/export.rs","text":hunk}},"checks":{rule:{"hunks":["h1"]}}},
        "questions":{format!("rule.{rule}"):{"type":"noul",
            "instructions":format!("Does hunk h1 violate rule {rule}?")}}})
    .to_string()
}
/// One reply per request: Some(noul) answers, None is a context rejection.
fn corpus(case: &str, answers: &[Option<f64>]) -> (Corpus, Vec<f64>, usize) {
    let mut corpus = Corpus {
        requests: vec![],
        replies: BTreeMap::new(),
    };
    let (mut expected, mut rejected) = (vec![], 0);
    for (index, answer) in answers.iter().enumerate() {
        let id = format!("phase1-{index}");
        let rule = format!("{case}-{index}");
        let body = body(
            &rule,
            &format!("@@ -1 +1 @@\n-old line {index}\n+new line {index}\n"),
        );
        let reply = match answer {
            Some(noul) => {
                expected.push(*noul);
                (
                    200,
                    json!({"model":"jev-1.13.0","answers":{format!("rule.{rule}"):{"type":"noul","noul":noul}},
                    "usage":{"input_tokens":120,"output_tokens":4}}),
                )
            }
            None => {
                rejected += 1;
                (
                    400,
                    json!({"error":{"message":"input exceeds the maximum context length"}}),
                )
            }
        };
        let tokens = Tokens::estimate(&body, None).unwrap();
        corpus
            .replies
            .insert(id.clone(), (reply.0, reply.1.to_string(), 120, 0));
        corpus.requests.push(WireRequest { id, body, tokens });
    }
    (corpus, expected, rejected)
}

#[test]
fn literal_cases_use_the_production_screen_with_identical_wakes() {
    let output = std::env::temp_dir().join(format!("option1-replay-{}", std::process::id()));
    kah_core::run_store::ensure_private_directory(&output).unwrap();
    let policy: DoubtLine =
        serde_json::from_str(include_str!("../../../assets/observer/commit-policy.json")).unwrap();
    let (below, above) = ((policy.line - 0.3).max(0.0), (policy.line + 0.1).min(1.0));
    let cases = [
        ("below-line", vec![Some(below), Some(0.0)]),
        ("at-or-above-line", vec![Some(below), Some(above)]),
        ("context-rejection", vec![Some(below), None]),
    ];
    for (case, answers) in cases {
        let (corpus, expected, rejected) = corpus(case, &answers);
        replay(&output, case, &corpus, &expected, rejected, &policy);
    }
    std::fs::remove_dir_all(output).unwrap();
}
fn replay(
    output: &Path,
    case: &str,
    corpus: &Corpus,
    expected: &[f64],
    rejected: usize,
    policy: &DoubtLine,
) {
    let stub = transport::Stub::start(corpus, false);
    let client = HttpClient::replay(stub.address).unwrap();
    let batch = Batch::new(&output.join(case).join("responses"), &output.join("rate")).unwrap();
    let screen = || evaluate("worker", case, policy, &batch, &corpus.requests, &client).unwrap();
    let record = screen();
    assert_eq!(record.woke, expected.iter().any(|n| *n >= policy.line));
    assert_eq!(record.gaps.len(), rejected);
    assert_eq!(record.answers.len(), expected.len());
    let count = stub.counts();
    let repeated = screen();
    assert_eq!(
        stub.counts(),
        count,
        "cached bodies, including the rejected one, are never resent"
    );
    assert_eq!(repeated.woke, record.woke);
}

#[test]
fn a_known_oversized_body_goes_directly_to_the_observer_after_whole_preparation() {
    use kah_observer::{
        context_budget::Calibration,
        context_selection::digest,
        whole_preparation::{self, Choice, Manifest, Outcome, Policy},
    };
    let body: Value = serde_json::from_str(&body(
        "no-network",
        "@@ -1 +1,2 @@\n fn export() {\n+    std::net::TcpStream::connect(\"example.invalid:80\").ok();\n",
    ))
    .unwrap();
    let catalogue = kah_cli::observer_review::option1::whole::catalogue(&body).unwrap();
    let selected = catalogue
        .sources
        .iter()
        .flat_map(|s| &s.records)
        .map(|r| Choice {
            id: r.id.clone(),
            digest: r.digest.clone(),
            reason: "keep-all terminal witness".into(),
        })
        .collect();
    let manifest = Manifest {
        body_digest: digest(&body),
        catalogue_digest: whole_preparation::catalogue_digest(&catalogue),
        selected,
        excluded: vec![],
    };
    let policy = Policy {
        calibration: Some(Calibration {
            input_utf8_bytes: 4,
            input_tokens: 1,
        }),
        known_oversized: std::collections::BTreeSet::from([digest(&body)]),
    };
    let Outcome::DirectObserverReview { gap } =
        whole_preparation::prepare(&body, &catalogue, &manifest, &policy).unwrap()
    else {
        panic!("a known rejection cannot return to Jev")
    };
    assert_eq!(gap.reason, "known_oversized");
}

#[test]
fn rule_placement_moves_each_rule_into_its_question() {
    let prepared = kah_observer::commit_check::Prepared {
        state: json!({"diff_hunks":{"h1":{"file":"src/export.rs","text":"+fn export() {}\n"}},
            "checks":{"r1":{"rule":"Never call the network.","hunks":["h1"],
                "permissions":[{"source":"user-1","start":0,"end":5,"quote":"Never"}]}}}),
        questions: serde_json::from_value(json!({"rule.r1":{"type":"noul",
            "instructions":"Does hunk h1 violate `checks.r1.rule`? Answer for this hunk only."}}))
        .unwrap(),
    };
    let after = kah_observer::commit_check::rule_in_question(&prepared).unwrap();
    let actual: Value = serde_json::from_str(&after.body().unwrap()).unwrap();
    let expected = json!({"model":"jev-1.13.0",
        "state":{"diff_hunks":{"h1":{"file":"src/export.rs","text":"+fn export() {}\n"}},
            "checks":{"r1":{"hunks":["h1"],
                "permissions":[{"source":"user-1","start":0,"end":5,"quote":"Never"}]}}},
        "questions":{"rule.r1":{"type":"noul",
            "instructions":"Does hunk h1 violate the rule \"Never call the network.\"? Answer for this hunk only."}}});
    assert_eq!(actual, expected);
}
