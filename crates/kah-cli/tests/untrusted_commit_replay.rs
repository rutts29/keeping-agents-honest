//! Retained real bodies through the Untrusted adapter, without network or login.
use kah_cli::observer_review::untrusted;
use kah_observer::{
    commit_review::DoubtLine,
    judge::batch::{Batch, RawResponse, Tokens, Transport, WireRequest},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Recorded {
    rows: BTreeMap<String, RawResponse>,
    calls: AtomicUsize,
}
impl Transport for Recorded {
    fn exchange(&self, request: &WireRequest) -> kah_observer::judge::batch::Result<RawResponse> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.rows[&request.id].clone())
    }
}
#[test]
#[ignore = "retained real corpus, credential-free $0 replay"]
fn retained_decisions_are_advisory_and_known_rejections_are_terminal_without_native_wakes() {
    let output = PathBuf::from(std::env::var_os("KAH_UNTRUSTED_REPLAY_OUTPUT").unwrap());
    kah_core::run_store::ensure_private_directory(&output).unwrap();
    let policy: DoubtLine =
        serde_json::from_str(include_str!("../../../assets/observer/commit-policy.json")).unwrap();
    let mut cases = BTreeMap::<String, Vec<Value>>::new();
    for root in [
        "synthetic-preparation",
        "synthetic-validation",
    ] {
        let path = Path::new("/mnt/kah-fixtures/cargo")
            .join(root)
            .join("jev/complete.json");
        let archive: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        for row in archive["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["job"]["stage"] == "literal")
        {
            cases
                .entry(row["job"]["case_id"].as_str().unwrap().into())
                .or_default()
                .push(row.clone());
        }
    }
    assert_eq!(cases.len(), 31);
    authority(&cases.values().next().unwrap()[0]);
    outages(&output, &cases["case-d15e9f532a44"], &policy);
    let results = cases
        .iter()
        .map(|(case, rows)| replay(&output, case, rows, &policy))
        .collect::<Vec<_>>();
    fs::write(output.join("adapter-proof.json"),serde_json::to_vec_pretty(&json!({"cases":results,"real_jev_calls":0,"native_calls":0,"claim":"Recorded transport/delivery mechanics; no new selection-quality or live-native gate claim"})).unwrap()).unwrap();
}
struct Outage {
    transport: Recorded,
    status: u16,
    failed: AtomicBool,
}
impl Transport for Outage {
    fn exchange(&self, request: &WireRequest) -> kah_observer::judge::batch::Result<RawResponse> {
        if request.id == "retained-1" && !self.failed.swap(true, Ordering::SeqCst) {
            self.transport.calls.fetch_add(1, Ordering::SeqCst);
            return Ok(RawResponse {
                status: self.status,
                headers: BTreeMap::new(),
                body: json!({"error":"recorded outage"}).to_string(),
            });
        }
        self.transport.exchange(request)
    }
}
fn outages(output: &Path, rows: &[Value], policy: &DoubtLine) {
    for status in [402, 408, 503] {
        let (requests, transport, expected, rejected) = corpus(rows);
        assert_eq!(requests.len(), 3);
        assert_eq!(rejected, 0);
        let outage = Outage {
            transport,
            status,
            failed: AtomicBool::new(false),
        };
        let root = output.join(format!("outage-{status}"));
        let batch = Batch::new(&root.join("responses"), &output.join("rate")).unwrap();
        let result = untrusted::evaluate(
            "worker",
            "d15e9f532a44948fe842e01e239a068d0367a80a",
            policy,
            &batch,
            &requests,
            json!({}),
            &outage,
        );
        assert!(result.is_err(), "outage must not become a terminal answer");
        assert_eq!(outage.transport.calls.load(Ordering::SeqCst), 3);
        let record = untrusted::evaluate(
            "worker",
            "d15e9f532a44948fe842e01e239a068d0367a80a",
            policy,
            &batch,
            &requests,
            json!({}),
            &outage,
        )
        .unwrap();
        assert_eq!(record.record.answers.len(), expected.len());
        assert_eq!(
            outage.transport.calls.load(Ordering::SeqCst),
            4,
            "only unanswered request retries"
        );
        fs::write(root.join("proof.json"),serde_json::to_vec_pretty(&json!({"http_status":status,"first":"pending outage","first_requests":3,"total_after_retry":4,"native_wake_count":0,"record":record})).unwrap()).unwrap();
    }
}
fn authority(row: &Value) {
    let root = Path::new(
        "/mnt/kah-fixtures/cargo/test-fixtures/synthetic-current-rules/native-home/repos/mine/rules-proof/input.json",
    );
    let source: kah_core::current_rules::Input =
        serde_json::from_slice(&fs::read(root).unwrap()).unwrap();
    let history = kah_core::intent_history::History::replay(source.history).unwrap();
    let path = Path::new(row["job"]["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("input.json");
    let bundle: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let original: kah_observer::commit_check::Input =
        serde_json::from_value(bundle["input"].clone()).unwrap();
    let context = json!({"head":original.revision,"baseline_tree":original.baseline,
        "touched_files":original.hunks.values().map(|h|json!({"path":h.file,"patch":h.text,"content":{"text":original.files[&h.file].text}})).collect::<Vec<_>>()});
    let input = untrusted::input::build(
        &history,
        &source
            .sources
            .iter()
            .filter(|s| s.origin["kind"] == "standing_file")
            .cloned()
            .collect::<Vec<_>>(),
        &context,
    )
    .unwrap();
    for item in history
        .snapshot()
        .intents
        .values()
        .filter(|i| i.is_active())
    {
        assert_eq!(input.intent.items[&item.id.to_string()].text, item.text);
    }
    for source in source
        .sources
        .iter()
        .filter(|s| s.origin["kind"] == "standing_file")
    {
        assert_eq!(input.intent.items[&source.id].text, source.text);
        assert_eq!(input.intent.items[&source.id].origin, "standing_file");
    }
    assert_eq!(input.carried["observer_session"], Value::Null);
    assert_eq!(
        kah_observer::commit_check::literal(&input)
            .unwrap()
            .questions
            .len(),
        input.intent.items.len()
    );
}
fn replay(output: &Path, case: &str, rows: &[Value], policy: &DoubtLine) -> Value {
    let (requests, transport, expected, rejected) = corpus(rows);
    let parent = source_directory(case, rows);
    let source = parent.join("whole-hunks/key-map.json");
    let map: Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
    let source = parent.join("input.json");
    let input: Value = serde_json::from_slice(&fs::read(source).unwrap()).unwrap();
    let head = input["input"]["revision"]
        .as_str()
        .or_else(|| input["revision"].as_str())
        .unwrap();
    let batch = Batch::new(&output.join(case).join("responses"), &output.join("rate")).unwrap();
    let record = untrusted::evaluate(
        "retained-worker",
        head,
        policy,
        &batch,
        &requests,
        map.clone(),
        &transport,
    )
    .unwrap();
    assert_eq!(record.flagged, expected.iter().any(|n| *n >= policy.line));
    assert_eq!(record.record.answers.len(), expected.len());
    assert_eq!(record.record.gaps.len(), rejected);
    assert!(!record.record.woke);
    assert!(record.record.native.is_none());
    assert_eq!(record.native_wake_count, 0);
    if record.flagged {
        delivery(output, case, &record);
    }
    assert!(
        record
            .record
            .gaps
            .iter()
            .all(|g| g["route"] == "recorded_untrusted_gap")
    );
    let notice = untrusted::notice(&record, "PostToolUse").unwrap();
    assert!(notice.get("decision").is_none());
    if record.flagged {
        assert!(
            notice["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains("Jev flag, not confirmed by an LLM")
        );
    }
    cached(&batch, &requests, &transport, &record, map);
    json!({"case":case,"report":record,"notice":notice,"cached_rejection_not_resent":true})
}
fn cached(
    batch: &Batch,
    requests: &[WireRequest],
    transport: &Recorded,
    record: &untrusted::Report,
    map: Value,
) {
    let count = transport.calls.load(Ordering::SeqCst);
    let again = untrusted::evaluate(
        &record.record.worker,
        &record.record.head,
        &record.record.policy,
        batch,
        requests,
        map,
        transport,
    )
    .unwrap();
    assert_eq!(transport.calls.load(Ordering::SeqCst), count);
    assert_eq!(json!(again), json!(record));
}
fn source_directory(case: &str, rows: &[Value]) -> PathBuf {
    let body = Path::new(rows[0]["job"]["path"].as_str().unwrap());
    if body.parent().unwrap().file_name().unwrap() == "whole-hunks" {
        body.parent().unwrap().parent().unwrap().into()
    } else {
        Path::new("/mnt/kah-fixtures/cargo/synthetic-packed-construction")
            .join(case)
            .join("all_changed_files")
    }
}
fn delivery(output: &Path, case: &str, report: &untrusted::Report) {
    for provider in ["codex", "claude"] {
        let root = output.join("delivery").join(case).join(provider);
        let notices = root.join("notices");
        kah_core::run_store::ensure_private_directory(&notices).unwrap();
        kah_cli::unattended_report::save(&notices, "notice.json", &json!(report)).unwrap();
        assert_eq!(untrusted::host::take_at(&root, "Stop").unwrap(), json!({}));
        assert!(!root.join("delivered/notice.json").exists());
        let output = untrusted::host::take_at(&root, "PostToolUse").unwrap();
        assert!(output.get("decision").is_none());
        assert!(
            output["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .unwrap()
                .contains(&report.record.head)
        );
        assert_eq!(
            untrusted::host::take_at(&root, "PostToolUse").unwrap(),
            json!({})
        );
    }
}
fn corpus(rows: &[Value]) -> (Vec<WireRequest>, Recorded, Vec<f64>, usize) {
    let mut requests = vec![];
    let mut replies = BTreeMap::new();
    let mut expected = vec![];
    let mut rejected = 0;
    for (index, row) in rows.iter().enumerate() {
        let body = fs::read_to_string(row["job"]["path"].as_str().unwrap()).unwrap();
        assert_eq!(
            kah_observer::commit_check::digest(body.as_bytes()),
            row["job"]["digest"]
        );
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
        let id = format!("retained-{index}");
        replies.insert(
            id.clone(),
            RawResponse {
                status,
                headers: BTreeMap::new(),
                body: serde_json::to_string(response).unwrap(),
            },
        );
        let tokens = Tokens::estimate(&body, None).unwrap();
        requests.push(WireRequest { id, body, tokens });
    }
    (
        requests,
        Recorded {
            rows: replies,
            calls: AtomicUsize::new(0),
        },
        expected,
        rejected,
    )
}
