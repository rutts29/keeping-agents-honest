//! Observer questions are answerable with the id they print, and one conflict
//! gives one question. Run through demos/check-tests.sh (isolated state root).
use kah_cli::observer_review::{
    option1::rule_questions,
    questions::{self, choice},
    state::{Purpose, Request, Store},
};
use kah_core::{
    current_rules::{Clause, Disposition, RuleSet, Words},
    intent_history::{GateDecision, QuestionAnswer, QuestionChoice, SourceOrigin, SourceProvider},
    run_store::RunStore,
};
use kah_observer::relayed_user::Message;
use serde_json::{Value, json};
use std::fs;

const DO_NOT: &str = "do not edit the root CHANGELOG.md (it is generated);";
const EVERY: &str = "Every change must add an entry to the root CHANGELOG.md.";

/// A worker with a private session store and review store, as a live run has.
fn worker() -> String {
    let worker = format!("choice-{}", kah_cli::relay_store::nonce());
    let base = RunStore::default_state_root().unwrap();
    drop(kah_observer::session::Store::open(&base, &worker, std::time::Duration::ZERO).unwrap());
    drop(Store::open(&worker).unwrap());
    worker
}
fn request(worker: &str) -> Request {
    Request {
        id: "review-1".into(),
        worker: worker.into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: json!({"revision":1,"goal":"Add a changelog line"}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    }
}
fn words(source: &str, quote: &str) -> Words {
    Words {
        source: source.into(),
        start: 0,
        end: quote.len(),
        quote: quote.into(),
    }
}
fn clause(id: &str, main: Words, witnesses: Vec<Words>) -> Clause {
    Clause {
        id: id.into(),
        words: main,
        disposition: Disposition::UnresolvedConflict,
        reason: "The goal and AGENTS.md disagree.".into(),
        rules: vec![],
        witnesses,
    }
}
/// One conflict can be reported by three clauses,
/// one of them listing the same goal text twice.
fn live_conflict() -> RuleSet {
    let standing = || words("standing:AGENTS.md", EVERY);
    RuleSet {
        schema_version: 1,
        input_digest: "sha256:input".into(),
        previous_digest: None,
        rules: vec![],
        clauses: vec![
            clause("c-user-1-6", words("goal:1", DO_NOT), vec![standing()]),
            clause(
                "c-user-2-6",
                words("intent:user-1", DO_NOT),
                vec![standing()],
            ),
            clause(
                "c-standing-changelog",
                standing(),
                vec![words("goal:1", DO_NOT), words("intent:user-1", DO_NOT)],
            ),
        ],
    }
}
fn stored_conflict(worker: &str) -> Value {
    let root = Store::directory(worker).unwrap();
    rule_questions::store(&root, &request(worker), &live_conflict()).unwrap();
    let mut pending = questions::pending(&root).unwrap();
    assert_eq!(pending.len(), 1, "one conflict must give one question");
    pending.remove(0)
}
/// The relayed user message, as the receiver hook accepts it.
fn relayed(worker: &str, receipt: &str, choice: QuestionChoice) -> kah_observer::inherited::Quote {
    let mut message = Message::new("operator", receipt, "Use the prompt's rule.").unwrap();
    message.choice = Some(choice);
    let decoded = Message::parse(&message.wire()).unwrap().unwrap();
    let base = RunStore::default_state_root().unwrap();
    decoded
        .accept(&base, worker, SourceProvider::Codex)
        .unwrap();
    decoded.quote(worker, SourceProvider::Codex).unwrap()
}

#[test]
fn three_requirement_ids_with_one_conflict_give_one_question_without_duplicate_options() {
    let w = worker();
    let question = stored_conflict(&w);
    assert_eq!(question["options"].as_array().unwrap().len(), 2);
    assert_eq!(
        question["requirements"],
        json!([
            "rule_conflict:c-user-1-6",
            "rule_conflict:c-user-2-6",
            "rule_conflict:c-standing-changelog"
        ])
    );
    let id = question["id"].as_str().unwrap();
    assert!(kah_core::intent_history::is_question_id(id));
    assert!(!id.contains(':'), "new ids use the one canonical form");
    // A later rebuild of the same conflict adds nothing.
    let root = Store::directory(&w).unwrap();
    rule_questions::store(&root, &request(&w), &live_conflict()).unwrap();
    assert_eq!(questions::pending(&root).unwrap().len(), 1);
    let message = questions::message(&question).unwrap();
    assert!(message.contains(&format!("kah steer {w} \"<the user's words>\" --question {id} --choose <option number or exact option text>")));
    assert!(message.contains("covers=["));
}

#[test]
fn option_choice_with_the_printed_id_closes_the_question_as_relayed_user_authority() {
    let w = worker();
    let question = stored_conflict(&w);
    let id = question["id"].as_str().unwrap();
    let relay = choice::Relay {
        worker: &w,
        question: id,
    };
    let by_text = relay.choice(DO_NOT).unwrap();
    let by_number = relay
        .choice(&(1 + position(&question, DO_NOT)).to_string())
        .unwrap();
    assert_eq!(by_text, by_number);
    let quote = relayed(&w, "receipt-choice", by_text.clone());
    assert_eq!(quote.source.origin, Some(SourceOrigin::Relayed));
    assert_eq!(quote.source.question_choice.as_ref(), Some(&by_text));
    let closed = choice::close(&w, &quote.source, &quote.text).unwrap();
    assert_eq!(closed["status"], "answered");
    assert_eq!(closed["resolution"]["choice"]["text"], DO_NOT);
    assert_eq!(closed["resolution"]["source"]["origin"], "relayed");
    assert_eq!(closed["resolution"]["source"]["role"], "user");
    assert_eq!(
        closed["resolution"]["requirements"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert!(
        questions::pending(&Store::directory(&w).unwrap())
            .unwrap()
            .is_empty()
    );
    // The user's words are in the session inbox as relayed user authority.
    let base = RunStore::default_state_root().unwrap();
    let store = kah_observer::session::Store::open(&base, &w, std::time::Duration::ZERO).unwrap();
    let inbox = store.inherited().unwrap().unwrap();
    assert!(
        inbox
            .quotes
            .iter()
            .any(|q| q.source.question_choice.as_ref() == Some(&by_text)
                && q.source.origin == Some(SourceOrigin::Relayed))
    );
    // The same relayed record again is idempotent; a closed question refuses a new choice.
    choice::close(&w, &quote.source, &quote.text).unwrap();
    assert!(relay.choice("1").is_err());
}

#[test]
fn legacy_printed_sha256_id_is_answerable() {
    let w = worker();
    let mut question = stored_conflict(&w);
    let legacy = format!("question-sha256:{}", "3f".repeat(32));
    question["id"] = legacy.clone().into();
    let directory = Store::directory(&w).unwrap().join("questions");
    fs::remove_dir_all(&directory).unwrap();
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    kah_cli::observer_review::source_store::save(&directory, &format!("{legacy}.json"), &question)
        .unwrap();
    QuestionAnswer {
        question: legacy.clone(),
        decision: GateDecision::Approve,
    }
    .validate()
    .unwrap();
    let relay = choice::Relay {
        worker: &w,
        question: &legacy,
    };
    assert!(
        relay
            .gate_only()
            .unwrap_err()
            .to_string()
            .contains("--choose")
    );
    let chosen = relay.choice("2").unwrap();
    let quote = relayed(&w, "receipt-legacy", chosen);
    assert_eq!(
        choice::close(&w, &quote.source, &quote.text).unwrap()["status"],
        "answered"
    );
}

#[test]
fn wrong_option_is_rejected_and_the_question_stays_open() {
    let w = worker();
    let question = stored_conflict(&w);
    let id = question["id"].as_str().unwrap();
    let relay = choice::Relay {
        worker: &w,
        question: id,
    };
    for wrong in ["0", "3", "use the generated one", " do not edit"] {
        assert!(
            relay
                .choice(wrong)
                .unwrap_err()
                .to_string()
                .contains("STEER_QUESTION_OPTION")
        );
    }
    let forged = QuestionChoice {
        question: id.into(),
        option: 1,
        text: "Some other instruction".into(),
    };
    let quote = relayed(&w, "receipt-forged", forged);
    let error = choice::close(&w, &quote.source, &quote.text).unwrap_err();
    assert!(error.to_string().contains("REVIEW_QUESTION_OPTION"));
    assert_eq!(
        questions::pending(&Store::directory(&w).unwrap())
            .unwrap()
            .len(),
        1
    );
    let bad = QuestionAnswer {
        question: "question-sha256:short".into(),
        decision: GateDecision::Approve,
    };
    let message = bad.validate().unwrap_err().to_string();
    assert!(message.contains("question-sha256:<64 hex>") && !message.contains("must not be empty"));
}

#[test]
fn gate_question_keeps_approve_reject_and_refuses_choose() {
    let w = worker();
    let id = format!("question-{}", "c".repeat(64));
    let directory = Store::directory(&w).unwrap().join("questions");
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    let gate = json!({"id":id,"kind":"checkpoint_approval","worker":w,"status":"pending","options":["approve","reject"]});
    kah_cli::observer_review::source_store::save(&directory, &format!("{id}.json"), &gate).unwrap();
    let relay = choice::Relay {
        worker: &w,
        question: &id,
    };
    relay.gate_only().unwrap();
    assert!(
        relay
            .choice("1")
            .unwrap_err()
            .to_string()
            .contains("--answer approve|reject")
    );
    for decision in ["approve", "reject"] {
        let mut message = Message::new("operator", "receipt-gate", "Approve it.").unwrap();
        message.answer = Some(QuestionAnswer {
            question: id.clone(),
            decision: GateDecision::parse(decision).unwrap(),
        });
        let wire = message.wire();
        assert!(wire.contains(&format!(" question={id} answer={decision}\n")));
        let decoded = Message::parse(&wire).unwrap().unwrap();
        assert_eq!(decoded.answer, message.answer);
        assert!(decoded.choice.is_none());
    }
}

#[test]
fn observer_question_options_are_distinct() {
    assert_eq!(
        questions::distinct_options(&["Exact".into(), " Exact ".into(), "Normalized".into()]),
        vec!["Exact".to_string(), "Normalized".to_string()]
    );
}

fn position(question: &Value, text: &str) -> usize {
    question["options"]
        .as_array()
        .unwrap()
        .iter()
        .position(|o| o == text)
        .unwrap()
}

/// The live v5 state: three stored legacy questions for one conflict.
fn stored_legacy(worker: &str) -> Vec<String> {
    let directory = Store::directory(worker).unwrap().join("questions");
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    let shapes = [
        ("3f", "c-user-1-6", json!([DO_NOT, EVERY])),
        ("69", "c-user-2-6", json!([DO_NOT, EVERY])),
        ("d5", "c-standing-changelog", json!([EVERY, DO_NOT, DO_NOT])),
    ];
    shapes
        .iter()
        .map(|(hex, clause, options)| {
            let id = format!("question-sha256:{}", hex.repeat(32));
            let q = json!({"id":id,"kind":"current_rule_conflict","worker":worker,"request":"review-1","requirement":format!("rule_conflict:{clause}"),"options":options,"status":"pending","delivery":"submitted","question":"Which instruction should govern this scope?"});
            kah_cli::observer_review::source_store::save(&directory, &format!("{id}.json"), &q)
                .unwrap();
            id
        })
        .collect()
}

#[test]
fn stored_duplicate_questions_fold_into_one_on_the_next_rebuild() {
    let w = worker();
    let ids = stored_legacy(&w);
    let root = Store::directory(&w).unwrap();
    rule_questions::store(&root, &request(&w), &live_conflict()).unwrap();
    let pending = questions::pending(&root).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0]["id"], ids[0]);
    assert_eq!(pending[0]["requirements"].as_array().unwrap().len(), 3);
    let chosen = choice::Relay {
        worker: &w,
        question: &ids[0],
    }
    .choice("1")
    .unwrap();
    let quote = relayed(&w, "receipt-fold", chosen);
    choice::close(&w, &quote.source, &quote.text).unwrap();
    assert!(questions::pending(&root).unwrap().is_empty());
}

#[test]
fn one_answer_closes_stored_duplicates_before_any_rebuild() {
    let w = worker();
    let ids = stored_legacy(&w);
    let chosen = choice::Relay {
        worker: &w,
        question: &ids[1],
    }
    .choice(EVERY)
    .unwrap();
    let quote = relayed(&w, "receipt-direct", chosen);
    let closed = choice::close(&w, &quote.source, &quote.text).unwrap();
    assert_eq!(
        closed["resolution"]["requirements"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let root = Store::directory(&w).unwrap();
    assert!(questions::pending(&root).unwrap().is_empty());
    let merged: Value = serde_json::from_str(
        &fs::read_to_string(root.join("questions").join(format!("{}.json", ids[0]))).unwrap(),
    )
    .unwrap();
    assert_eq!(merged["status"], "merged");
    assert_eq!(merged["merged_into"], ids[1]);
}
