//! Observer report validation reads only the job parts it checks; rejections stay exact.
use kah_cli::observer_review::{
    option1::{self, input, protocol},
    source_store,
    state::{Purpose, Request},
};
use kah_core::intent_history::{
    Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::{
    commit_check::{Input, Rule},
    review_evidence::Document,
    rule_source::{Link, Resolved, Rule as SourceRule},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
const FILES: usize = 4;
const ID: &str = "review-selection-test";
struct Fixture {
    root: PathBuf,
    request_path: PathBuf,
    request: Request,
    input: Input,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn history() -> History {
    let source = SourceRef::new(
        SourceProvider::Codex,
        "native-test",
        "u1",
        SourceRole::User,
        "sha256:test",
    );
    let mut h = History::new(GoalRevision::new(
        1,
        "Preserve complete evidence.",
        vec![source.clone()],
    ))
    .unwrap();
    h.append(Event::RequirementAdded {
        item: IntentItem::new(
            "user-source",
            IntentKind::Constraint,
            "Never cut evidence.",
            vec![source],
        ),
        approval: None,
    })
    .unwrap();
    h
}
fn rules() -> Resolved {
    Resolved {
        source_digest: "source-binding".into(),
        observer_session: "same-native".into(),
        source_words: Default::default(),
        history_digest: None,
        rules: vec![SourceRule {
            id: "rule-whole".into(),
            predicate: "Never cut evidence.".into(),
            diff_checkable: true,
            kind: Default::default(),
            scope: String::new(),
            permissions: vec![],
            sources: vec![Link {
                source_id: "user-source".into(),
                quote: "Never cut evidence.".into(),
            }],
        }],
    }
}
fn context() -> Value {
    let files: Vec<Value> = (1..=FILES)
        .map(|n| {
            json!({"path":format!("f{n}.rs"),"deleted":false,"patch":format!("+patch_{n}();\n"),
                "content":{"text":format!("fn file_{n}() {{}}\nkeep_all_{n}();\n")}})
        })
        .collect();
    json!({"head":"a".repeat(40),"baseline_tree":"b".repeat(40),"touched_files":files})
}
fn fixture(name: &str) -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "kah-option1-selection-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    let directory = root.join("inputs").join(ID);
    for path in [root.join("sources"), root.join("outbox"), directory.clone()] {
        fs::create_dir_all(path).unwrap();
    }
    let mut input = input::build(&history(), &rules(), &context(), true).unwrap();
    let hunks = input.hunks.keys().cloned().collect();
    let pending = Rule {
        authority: "controller_intent_pending".into(),
        unconditional_prohibition: false,
        prohibition: false,
        literal_check: false,
        hunks,
    };
    input
        .rules
        .insert("controller_intent_pending".into(), pending);
    let job = json!({"stage":"checkpoint","input":input});
    let request = Request {
        id: ID.into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some(input.revision.clone()),
        intent: Value::Null,
        trigger: json!({ "option1": job }),
        documents: vec![
            Document::new(
                "option1",
                "literal_first",
                "kah:option1",
                &serde_json::to_string(&job).unwrap(),
            ),
            Document::new(
                "history",
                "native_history",
                "kah:history",
                "whole native log",
            ),
        ],
        created_ms: 0,
    };
    let value = serde_json::to_value(&request).unwrap();
    source_store::save(&directory, "request.json", &value).unwrap();
    Fixture {
        request_path: directory.join("request.json"),
        root,
        request,
        input,
    }
}
fn digest(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn stored(root: &Path, value: &Value) -> Value {
    let id = value["$kah_source"].as_str().unwrap();
    serde_json::from_slice(&fs::read(root.join("sources").join(id)).unwrap()).unwrap()
}
/// One row per rule; the first row quotes two whole lines of f2.rs.
fn report(input: &Input) -> Value {
    let file = &input.files["f2.rs"];
    let rows: Vec<Value> = input
        .rules
        .keys()
        .enumerate()
        .map(|(n, id)| {
            let quotes = if n == 0 {
                json!([{"file":"f2.rs","first_line":1,"last_line":2,"text":file.text,"digest":file.digest}])
            } else {
                json!([])
            };
            let status = if id == "controller_intent_pending" { "unclear" } else { "satisfied" };
            json!({"requirement":id,"finding":"checked","status":status,"kind":"behavior","quotes":quotes})
        })
        .collect();
    json!({"request":ID,"result":{"stage":"checkpoint","rows":rows}})
}
fn submit(f: &Fixture, report: &Value) -> (Result<(), String>, Vec<String>) {
    let path = f.root.join("outbox").join(format!("{ID}.json"));
    fs::write(&path, serde_json::to_vec(report).unwrap()).unwrap();
    let args: Vec<String> = [
        "observe",
        "report",
        "--request",
        f.request_path.to_str().unwrap(),
        "--json",
        path.to_str().unwrap(),
    ]
    .map(str::to_owned)
    .to_vec();
    let (result, reads) = source_store::trace::record(|| option1::workspace::report(&args));
    (result.map_err(|e| e.to_string()), reads)
}
fn complete(f: &Fixture, report: &Value) -> Result<(), String> {
    protocol::validate(&f.request, &serde_json::to_vec(report).unwrap())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[test]
fn cited_files_validate_without_unrelated_files_patches_documents_or_second_job_read() {
    let f = fixture("cited");
    let request: Value = serde_json::from_slice(&fs::read(&f.request_path).unwrap()).unwrap();
    let trigger = stored(&f.root, &request["trigger"]);
    let job = trigger["option1"]["$kah_source"]
        .as_str()
        .unwrap()
        .to_owned();
    let document = stored(&f.root, &request["documents"][0]);
    assert_eq!(
        document["text"]["$kah_source"],
        job.as_str(),
        "one job, two references"
    );
    assert_eq!(document["text"]["encoding"], "json_text");
    let ((), full) = source_store::trace::record(|| {
        source_store::read::<Request>(&f.request_path)
            .map(|_| ())
            .unwrap()
    });
    assert_eq!(
        full.iter().filter(|d| **d == job).count(),
        2,
        "old reader expands both"
    );
    let (result, reads) = submit(&f, &report(&f.input));
    result.unwrap();
    assert_eq!(
        reads.iter().filter(|d| **d == job).count(),
        1,
        "job selected once"
    );
    let wrapper = request["documents"][0]["$kah_source"].as_str().unwrap();
    assert!(!reads.iter().any(|d| d == wrapper), "documents stay unread");
    assert!(
        reads.contains(&digest(&f.input.files["f2.rs"].text)),
        "cited file is whole"
    );
    for n in (1..=FILES).filter(|n| *n != 2) {
        let text = &f.input.files[&format!("f{n}.rs")].text;
        assert!(full.contains(&digest(text)), "old reader expanded f{n}.rs");
        assert!(
            !reads.contains(&digest(text)),
            "unrelated f{n}.rs stays unread"
        );
    }
    for hunk in f.input.hunks.values() {
        assert!(full.contains(&digest(&hunk.text)));
        assert!(!reads.contains(&digest(&hunk.text)), "patches stay unread");
    }
    for id in f.input.hunks.keys() {
        assert!(
            !reads.contains(&digest(id)),
            "rule hunk memberships stay unread"
        );
    }
    accepted_like_complete(&f);
}
fn accepted_like_complete(f: &Fixture) {
    let accepted: Value = serde_json::from_slice(
        &fs::read(f.root.join(format!("outbox/submitted-{ID}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(accepted, report(&f.input));
    assert!(complete(f, &report(&f.input)).is_ok());
}

type Change = fn(&mut Value);
fn rows(report: &mut Value) -> &mut Vec<Value> {
    report["result"]["rows"].as_array_mut().unwrap()
}
fn quote(report: &mut Value) -> &mut Value {
    &mut rows(report)[0]["quotes"][0]
}
fn row_named<'a>(report: &'a mut Value, id: &str) -> &'a mut Value {
    rows(report)
        .iter_mut()
        .find(|row| row["requirement"] == id)
        .unwrap()
}

#[test]
fn every_rejection_keeps_its_reason_code_through_selected_validation() {
    let f = fixture("rejections");
    let cases: Vec<(&str, Change)> = vec![
        ("COMMIT_REPORT_SCOPE", |r| {
            rows(r).pop();
        }),
        ("COMMIT_REPORT_SCOPE", |r| {
            let first = rows(r)[0].clone();
            rows(r).push(first);
        }),
        ("COMMIT_REPORT_SCOPE", |r| {
            rows(r)[0]["requirement"] = json!("unknown-rule");
        }),
        ("MEASURE_QUOTE_DIGEST", |r| {
            quote(r)["digest"] = json!("sha256:0")
        }),
        ("MEASURE_QUOTE_TEXT", |r| {
            quote(r)["text"] = json!("not in file\n")
        }),
        ("MEASURE_QUOTE_RANGE", |r| quote(r)["last_line"] = json!(9)),
        ("MEASURE_QUOTE_RANGE", |r| quote(r)["first_line"] = json!(0)),
        ("MEASURE_QUOTE_FILE", |r| {
            quote(r)["file"] = json!("absent.rs")
        }),
        ("COMMIT_INTENT_NOT_ACCEPTED", |r| {
            row_named(r, "controller_intent_pending")["status"] = json!("satisfied");
        }),
        ("COMMIT_REPORT_ABSENCE_SCOPE", |r| {
            row_named(r, "goal")["kind"] = json!("not_violated");
        }),
        ("COMMIT_REPORT_QUOTES", |r| {
            row_named(r, "goal")["status"] = json!("violated");
        }),
        ("COMMIT_REPORT_BINDING", |r| {
            r["request"] = json!("review-other")
        }),
        ("COMMIT_REPORT_STAGE", |r| {
            r["result"] = json!({"stage":"rules","rules":[]});
        }),
    ];
    for (code, change) in cases {
        let mut bad = report(&f.input);
        change(&mut bad);
        let (selected, _) = submit(&f, &bad);
        let selected = selected.unwrap_err();
        let full = complete(&f, &bad).unwrap_err();
        assert!(selected.starts_with(code), "{code}: {selected}");
        assert_eq!(selected, full, "{code}: same rejection as the complete job");
    }
}

#[test]
fn selection_stage_reads_only_the_stage() {
    let f = fixture("stage");
    let mut job = serde_json::to_value(&f.request).unwrap();
    job["trigger"]["option1"] = json!({"stage":"selection","input":f.input});
    let request: Request = serde_json::from_value(job.clone()).unwrap();
    let directory = f.request_path.parent().unwrap();
    source_store::save(directory, "request.json", &job).unwrap();
    let report = json!({"request":ID,"result":{"stage":"selection","manifests":[]}});
    let (result, reads) = submit(&f, &report);
    result.unwrap();
    assert!(protocol::validate(&request, &serde_json::to_vec(&report).unwrap()).is_ok());
    for file in f.input.files.values() {
        assert!(!reads.contains(&digest(&file.text)));
    }
}
