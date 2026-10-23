//! Read-only count of frozen native text. No judge, control client or native session.
use kah_core::intent_history::SourceRole;
use kah_observer::{
    context_budget::Calibration,
    inherited::Inbox,
    judge::{Questions, questions},
    relayed_user::Message,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "phase3 $0 catch-up count; explicit frozen private input/output"]
fn count_real_revision_twenty_eight_catchup() {
    let root = PathBuf::from(std::env::var_os("KAH_PHASE3_COUNT_ROOT").unwrap());
    let raw = fs::read(root.join("catchup-inputs.json")).unwrap();
    let captured: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(captured["projection"]["revision"], 28);
    assert_eq!(captured["pending_tail_bytes"], 0);
    let inbox: Inbox = serde_json::from_value(captured["inbox"].clone()).unwrap();
    inbox.validate().unwrap();
    let (counts, records, calibration) = scan(&root, &captured, &inbox);
    let accepted = captured["accepted_cursor"]["accepted_records"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect::<BTreeSet<_>>();
    let pending_inbox = inbox
        .quotes
        .iter()
        .filter(|q| !accepted.contains(q.key().as_str()))
        .collect::<Vec<_>>();
    assert!(
        pending_inbox.is_empty(),
        "Count unprocessed inbox quotes separately; never infer they are already typed from transcript absence"
    );
    let requests = records
        .iter()
        .filter(|r| r["jev_typing_request"] == true)
        .count();
    let baseline = records
        .iter()
        .filter_map(|r| r["request"]["plan"]["estimated_request_tokens"].as_f64())
        .sum::<f64>();
    let unresolved = records
        .iter()
        .filter(|r| r["classification"] == "relayed_user_host_acceptance_unsettled")
        .count();
    let value = json!({"zero_jev":true,"zero_native":true,"input_digest":kah_observer::commit_check::digest(&raw),"prefix_bytes":captured["prefix_bytes"],"prefix_sha256":captured["prefix_sha256"],"cutoff":captured["cutoff"],"revision":28,"native_text_parts":records.len(),"counts":counts,"accepted_inbox_quotes":inbox.quotes.len(),"accepted_source_cursor_records":accepted.len(),"pending_inbox_quotes":pending_inbox.len(),"typing_requests":requests,"baseline_estimated_tokens":baseline,"baseline_estimated_usd":baseline*0.042/1_000_000.0,"documented_request_reservation_tokens":requests*64000,"reservation_usd":requests as f64*64000.0*0.042/1_000_000.0,"unsettled_host_acceptance":unresolved,"native_rule_rebuild":{"turns":1,"jev_calls":0,"scope":"One coalesced rebuild after catch-up settles; correction/uncertain-steering turns are conditional, not claimed to have run"},"calibration":calibration,"records":records,"future_prior_warning":"Bodies show actual verbatim words against the frozen revision28 prior ONLY. Later accepted changes alter subsequent priors. No future body is claimed exact; reserve the documented64K per typing request, never use it as a content cap."});
    fs::write(
        root.join("catchup-count.json"),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
    println!(
        "{}",
        json!({"counts":value["counts"],"typing_requests":requests,"baseline_estimated_tokens":baseline,"reservation_usd":value["reservation_usd"],"unsettled":unresolved})
    );
}
fn scan(
    root: &Path,
    captured: &Value,
    inbox: &Inbox,
) -> (BTreeMap<String, usize>, Vec<Value>, Calibration) {
    let prior = &captured["projection"];
    let items = prior["preserve"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(prior["frontier"].as_array().into_iter().flatten())
        .chain(prior["validation"].as_array().into_iter().flatten())
        .map(|i| {
            (
                i["id"].as_str().unwrap().to_owned(),
                i["text"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    let calibration_source: Value =
        serde_json::from_slice(&fs::read(root.join("calibration.json")).unwrap()).unwrap();
    let calibration = Calibration {
        input_utf8_bytes: 1_000_000,
        input_tokens: (calibration_source["calibration"]["tokens_per_byte"]
            .as_f64()
            .unwrap()
            * 1_000_000.0)
            .round() as u64,
    };
    let mut counts = BTreeMap::<String, usize>::new();
    let mut seen = BTreeSet::new();
    let mut records = vec![];
    for row in captured["rows"].as_array().unwrap() {
        let (class, text, key) = classify(row, inbox);
        *counts.entry(class.clone()).or_default() += 1;
        let duplicate = key.is_some_and(|k| !seen.insert(k));
        let api = matches!(class.as_str(), "user" | "relayed_user_host_accepted")
            && !duplicate
            && !explicit(&text);
        let request = if api {
            Some(render(root, row, &text, prior, &items, &calibration))
        } else {
            None
        };
        records.push(json!({"timestamp":row["timestamp"],"offset":row["offset"],"part":row["part"],"classification":class,"duplicate_receipt":duplicate,"jev_typing_request":api,"request":request,"text_digest":kah_observer::commit_check::digest(text.as_bytes())}));
    }
    (counts, records, calibration)
}
fn explicit(text: &str) -> bool {
    let first = text.trim_start().get(..6).unwrap_or("");
    first.eq_ignore_ascii_case("steer:") || first.eq_ignore_ascii_case("aside:")
}
fn classify(row: &Value, inbox: &Inbox) -> (String, String, Option<String>) {
    let text = row["text"].as_str().unwrap();
    if let Some(message) = Message::parse(text).unwrap() {
        let accepted = inbox.quotes.iter().any(|q| {
            q.receipt == message.receipt
                && q.text == message.text
                && q.source.relayer.as_deref() == Some(&message.relayer)
        });
        return (
            (if accepted {
                "relayed_user_host_accepted"
            } else {
                "relayed_user_host_acceptance_unsettled"
            })
            .into(),
            message.text,
            Some(message.receipt),
        );
    }
    let role = kah_codex::authorship::with_client_id(text, row["client_id"].as_str());
    let class = match role {
        SourceRole::User => "user",
        SourceRole::Delegate => "delegate",
        _ => "system_observation",
    };
    (class.into(), text.into(), None)
}
fn render(
    root: &Path,
    row: &Value,
    text: &str,
    prior: &Value,
    items: &[(String, String)],
    calibration: &Calibration,
) -> Value {
    let state = questions::steering_state(prior["goal"]["text"].as_str(), None, items, text, "");
    let questions: Questions = questions::steering(items);
    let body = json!({"model":"jev-1.13.0","state":state,"questions":questions});
    let bytes = serde_json::to_vec(&body).unwrap();
    let digest = kah_observer::commit_check::digest(&bytes);
    let path = root.join(format!("intake-{}-{}.json", row["offset"], row["part"]));
    fs::write(&path, &bytes).unwrap();
    json!({"path":path,"digest":digest,"question_keys":questions.keys().collect::<Vec<_>>(),"plan":calibration.plan(&state,&questions).unwrap(),"scope":"Frozen-prior count template, not future sequential typing body"})
}
#[test]
fn plain_reviewer_prefix_has_no_user_authority() {
    let row = json!({"text":"[kah-reviewer] A decision relay without the user envelope.","client_id":null});
    let inbox = Inbox {
        provider: kah_core::intent_history::SourceProvider::Codex,
        quotes: vec![],
    };
    assert_eq!(classify(&row, &inbox).0, "delegate");
}
