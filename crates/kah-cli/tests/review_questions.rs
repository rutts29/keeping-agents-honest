use kah_cli::observer_review::{
    questions,
    state::{Purpose, Request},
};
use kah_observer::{decision_units::ReviewUnits, review_evidence::Report};
use serde_json::json;

fn request() -> Request {
    Request {
        id: "review-first".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some("sha".into()),
        intent: json!({"revision":1}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    }
}
fn report() -> Report {
    let units:ReviewUnits=serde_json::from_value(json!({"requirements":[{"id":"format","name":"Output format","dimension":"intent","authority_items":["goal"],"citations":[],"components":[],"checks":[],"judgment":{"kind":"needs_user","reason":"ambiguous_instructions","summary":"Exact and normalized output are both plausible.","question":"Which output format should apply?","options":["Exact","Normalized"],"evidence_citations":[0],"ledger_review":{"reasoning":"No prior format decision resolves this.","citations":[0]}}}],"proportion":null})).unwrap();
    let mut r: Report =
        serde_json::from_value(json!({"request_id":"review-first","findings":[]})).unwrap();
    r.decision_units = Some(units);
    r
}
#[test]
fn named_question_is_durable_deduplicated_and_scoped_to_one_requirement() {
    let root = std::env::temp_dir().join(format!("kah-question-unit-{}", std::process::id()));
    let first = questions::store(
        &root,
        &request(),
        &report(),
        &json!({"needs_user":["format"],"needs_thinker":[]}),
    )
    .unwrap();
    assert_eq!(first.len(), 1);
    let mut later = request();
    later.id = "review-later".into();
    assert!(
        questions::store(&root, &later, &report(), &json!({"needs_user":["format"]}))
            .unwrap()
            .is_empty()
    );
    let pending = questions::pending(&root).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["requirement"], "format");
    assert!(
        questions::message(&pending[0])
            .unwrap()
            .starts_with("[kah-builder] [kah-question]")
    );
    assert!(
        questions::message(&pending[0])
            .unwrap()
            .contains("Unrelated authorized work can continue")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn pending_question_requires_a_new_user_source_and_jev_validation_to_resolve() {
    use kah_observer::judge::{Answer, Judgment, Metrics};
    use std::collections::BTreeMap;
    let root = std::env::temp_dir().join(format!("kah-question-answer-{}", std::process::id()));
    let first = request();
    questions::store(&root, &first, &report(), &json!({"needs_user":["format"]})).unwrap();
    let mut answer = request();
    answer.id = "review-answer".into();
    answer.intent = json!({"goal":"Original", "items":[{"id":"user-answer","text":"Use exact output.","source":[{"role":"assistant"}]}]});
    let r = answered_report();
    let mut judgment = Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::new(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    judgment.answers.insert(
        "requirement.format".into(),
        Answer::Choice {
            choice: "supported".into(),
            confidence: 0.95,
            probabilities: BTreeMap::from([("supported".into(), 0.95)]),
        },
    );
    assert_eq!(
        questions::resolve(&root, &answer, &r, &judgment).unwrap(),
        0
    );
    answer.intent["items"][0]["source"][0]["role"] = "user".into();
    assert_eq!(
        questions::resolve(
            &root,
            &answer,
            &r,
            &Judgment {
                backend: "not_requested".into(),
                ..judgment.clone()
            }
        )
        .unwrap(),
        0
    );
    assert_eq!(
        questions::resolve(&root, &answer, &r, &judgment).unwrap(),
        1
    );
    assert!(questions::pending(&root).unwrap().is_empty());
    let path = std::fs::read_dir(root.join("questions"))
        .unwrap()
        .find_map(|e| {
            let p = e.unwrap().path();
            (p.extension().is_some_and(|x| x == "json")).then_some(p)
        })
        .unwrap();
    let saved: serde_json::Value = kah_cli::observer_review::state::read(&path)
        .unwrap()
        .unwrap();
    assert_eq!(saved["status"], "answered");
    assert_eq!(saved["resolution"]["authority"]["id"], "user-answer");
    std::fs::remove_dir_all(root).unwrap();
}
fn answered_report() -> Report {
    let mut r = report();
    let unit = &mut r.decision_units.as_mut().unwrap().requirements[0];
    unit.authority_items.push("user-answer".into());
    unit.judgment = kah_observer::decision_units::Thinking::Semantic {
        decision: kah_observer::decision_units::SemanticDecision::Satisfied,
        reasoning: "The user resolved the format.".into(),
        evidence_citations: vec![0],
    };
    unit.citations = vec![
        serde_json::from_value(
            json!({"source":"ledger","digest":"sha256:unused","quote":"Use exact output."}),
        )
        .unwrap(),
    ];
    r
}

#[test]
fn grouped_gate_question_cannot_be_resolved_by_semantic_user_interpretation() {
    use kah_observer::judge::{Answer, Judgment, Metrics};
    use std::collections::BTreeMap;
    let root = std::env::temp_dir().join(format!("kah-grouped-answer-{}", std::process::id()));
    let saved = questions::store(
        &root,
        &request(),
        &report(),
        &json!({"needs_user":["format"]}),
    )
    .unwrap();
    let mut question = saved[0].clone();
    question["kind"] = "checkpoint_approval".into();
    let path = root
        .join("questions")
        .join(format!("{}.json", question["id"].as_str().unwrap()));
    std::fs::write(path, serde_json::to_vec(&question).unwrap()).unwrap();
    let mut answer = request();
    answer.intent = json!({"goal":"Original","items":[{"id":"user-answer","text":"Use exact output.","source":[{"role":"user","origin":"relayed"}]}]});
    let judgment = Judgment {
        backend: "jev".into(),
        model: "jev-latest".into(),
        answers: BTreeMap::from([(
            "requirement.format".into(),
            Answer::Choice {
                choice: "supported".into(),
                confidence: 0.95,
                probabilities: BTreeMap::from([("supported".into(), 0.95)]),
            },
        )]),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    assert_eq!(
        questions::resolve(&root, &answer, &answered_report(), &judgment).unwrap(),
        0
    );
    assert_eq!(questions::pending(&root).unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn answer_detection_preserves_old_authority_and_includes_a_new_user_goal() {
    let question = json!({"authority_at_question":["goal","old"],"goal_at_question":"Original"});
    let intent = json!({"goal":"Use exact output.","goal_source":[{"role":"user","origin":"relayed"}],"items":[{"id":"old","text":"Old decision","source":[{"role":"user"}]},{"id":"agent-new","text":"I choose normalized","source":[{"role":"assistant"}]}]});
    let candidates = questions::new_user_items(&question, &intent);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0]["id"], "goal");
    assert_eq!(candidates[0]["text"], "Use exact output.");
}

#[test]
fn a_worker_asked_question_is_relayed_verbatim_and_named_as_the_workers_question() {
    let native_question = "Preserve CSV quoting or normalize it?";
    let mut question = serde_json::json!({"id":"q-native","worker":"worker","requirement":"policy","question":native_question,"options":["Preserve","Normalize"],"summary":"The user has not chosen.","ledger_review":{"reasoning":"No answer exists."},"question_origin":"worker","worker_question_source":{"source":"transcript","digest":"sha256:source","occurrence":0}});
    let message = questions::message(&question).unwrap();
    assert!(
        message.contains("Worker's question (verbatim):\nPreserve CSV quoting or normalize it?\n")
    );
    question["question_origin"] = "observer".into();
    assert!(
        !questions::message(&question)
            .unwrap()
            .contains("Worker's question (verbatim)")
    );
}

#[test]
fn native_question_keeps_one_durable_id_when_transcript_and_option_labels_change() {
    use kah_observer::review_evidence::{Document, NativeRecord};
    let root = std::env::temp_dir().join(format!("kah-native-question-{}", std::process::id()));
    let ledger = Document::new(
        "ledger",
        "ledger",
        "kah:ledger",
        "No CSV quoting decision exists.",
    );
    let text = "Preserve CSV quoting or normalize it?";
    let native = |suffix: &str| {
        let mut doc = Document::new(
            "transcript",
            "worker_transcript",
            "native:worker",
            &format!("{text}{suffix}"),
        );
        doc.native_records = vec![NativeRecord {
            start_byte: 0,
            end_byte: text.len(),
            context: json!({"record_type":"agentMessage","record_id":"native-question-1"}),
        }];
        doc
    };
    let mut request = request();
    request.intent = json!({"goal":"Build CSV formatter."});
    request.documents = vec![ledger.clone(), native("")];
    let mut report = report();
    let mut raw = serde_json::to_value(report.decision_units.take().unwrap()).unwrap();
    raw["version"] = json!(7);
    raw["requirements"][0]["blocker"] = json!("user_decision");
    raw["requirements"][0]["worker_question"] = json!(1);
    raw["requirements"][0]["judgment"]["question"] = json!(text);
    raw["requirements"][0]["citations"] = json!([
        {"source":"ledger","digest":ledger.digest,"quote":ledger.text},
        {"source":"transcript","digest":request.documents[1].digest,"quote":text}
    ]);
    report.decision_units = Some(serde_json::from_value(raw.clone()).unwrap());
    let assessment = json!({"needs_user":["format"]});
    let saved = questions::store(&root, &request, &report, &assessment).unwrap();
    assert_eq!(saved[0]["question_origin"], "worker");
    assert_eq!(saved[0]["question"], text);
    assert_eq!(
        saved[0]["worker_question_source"]["native_records"][0]["record_id"],
        "native-question-1"
    );
    request.id = "review-next".into();
    request.documents[1] = native("\nReadiness work continued.");
    raw["requirements"][0]["citations"][1]["digest"] = json!(request.documents[1].digest);
    raw["requirements"][0]["judgment"]["options"] =
        json!(["Keep original quoting", "Use canonical quoting"]);
    report.decision_units = Some(serde_json::from_value(raw).unwrap());
    assert!(
        questions::store(&root, &request, &report, &assessment)
            .unwrap()
            .is_empty()
    );
    assert_eq!(questions::pending(&root).unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}
