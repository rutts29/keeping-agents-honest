use kah_cli::observer_review::option1::{input, protocol, requests};
use kah_core::intent_history::{
    Event, GoalRevision, History, IntentItem, IntentKind, SourceProvider, SourceRef, SourceRole,
};
use kah_observer::{
    commit_check::Status,
    commit_review,
    rule_source::{Link, Resolved, Rule},
};
use serde_json::{Value, json};
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
fn source() -> Resolved {
    Resolved {
        source_digest: "source-binding".into(),
        observer_session: "same-native".into(),
        rules: vec![Rule {
            id: "rule-whole".into(),
            predicate: "Never cut evidence.".into(),
            diff_checkable: true,
            sources: vec![Link {
                source_id: "user-source".into(),
                quote: "Never cut evidence.".into(),
            }],
        }],
    }
}
fn context() -> Value {
    json!({"head":"a".repeat(40),"baseline_tree":"b".repeat(40),"touched_files":[{"path":"a.rs","deleted":false,"patch":"+keep_all();\n","content":{"text":"keep_all();\n"}}]})
}
#[test]
fn commit_has_literal_only_and_gate_keeps_the_same_authority_identity() {
    let h = history();
    let commit = input::build(&h, &source(), &context(), false).unwrap();
    let gate = input::build(&h, &source(), &context(), true).unwrap();
    assert_eq!(
        commit_review::authority(&commit),
        commit_review::authority(&gate)
    );
    assert!(!commit.rules.contains_key("goal"));
    assert!(gate.rules.contains_key("goal"));
    let (bodies, map) = requests::build(&commit, None).unwrap();
    assert_eq!(bodies.len(), 1);
    let body: Value = serde_json::from_str(&bodies[0].body).unwrap();
    assert!(
        body["questions"]
            .as_object()
            .unwrap()
            .values()
            .all(|q| q["type"] == "noul")
    );
    assert!(body["state"]["rows"].is_null());
    assert_eq!(map["rules"].as_object().unwrap().len(), 1);
    assert_eq!(body["state"]["diff_hunks"]["h1"]["text"], "+keep_all();\n");
}
#[test]
fn native_confirmation_cannot_skip_a_rule_or_forge_a_whole_quote() {
    use kah_cli::observer_review::state::{Purpose, Request};
    let input = input::build(&history(), &source(), &context(), false).unwrap();
    let request = Request {
        id: "review-test".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: Some(input.revision.clone()),
        intent: Value::Null,
        trigger: json!({"option1":{"stage":"confirmation","input":input}}),
        documents: vec![],
        created_ms: 0,
    };
    let missing = json!({"request":"review-test","result":{"stage":"confirmation","rows":[]}});
    assert!(protocol::validate(&request, &serde_json::to_vec(&missing).unwrap()).is_err());
    let mut good = json!({"request":"review-test","result":{"stage":"confirmation","rows":[{"requirement":"rule-whole","finding":"No cut introduced","status":"satisfied","kind":"not_violated","quotes":[]}]}});
    assert!(protocol::validate(&request, &serde_json::to_vec(&good).unwrap()).is_ok());
    good["result"]["rows"][0]["status"] = json!(Status::Violated);
    assert!(protocol::validate(&request, &serde_json::to_vec(&good).unwrap()).is_err());
}
#[test]
fn known_oversized_whole_patch_routes_direct_without_rendering_a_fake_small_body() {
    use kah_observer::{
        context_budget::Calibration,
        context_selection::digest,
        whole_preparation::{self, Choice, Manifest, Outcome, Policy},
    };
    let input = input::build(&history(), &source(), &context(), false).unwrap();
    let (requests, _) = requests::build(&input, None).unwrap();
    let body: Value = serde_json::from_str(&requests[0].body).unwrap();
    let catalogue = kah_cli::observer_review::option1::whole::catalogue(&body).unwrap();
    let record = &catalogue.sources[0].records[0];
    let manifest = Manifest {
        body_digest: digest(&body),
        catalogue_digest: whole_preparation::catalogue_digest(&catalogue),
        selected: vec![Choice {
            id: record.id.clone(),
            digest: record.digest.clone(),
            reason: "whole changed patch is relevant".into(),
        }],
        excluded: vec![],
    };
    let policy = Policy {
        calibration: Some(Calibration {
            input_utf8_bytes: 10,
            input_tokens: 1,
        }),
        known_oversized: std::collections::BTreeSet::from([digest(&body)]),
    };
    let Outcome::DirectObserverReview { gap } =
        whole_preparation::prepare(&body, &catalogue, &manifest, &policy).unwrap()
    else {
        panic!("must not resend known oversized body")
    };
    assert_eq!(gap.reason, "known_oversized");
    assert_eq!(gap.original_body_digest, gap.candidate_body_digest);
}
#[test]
fn native_prompt_uses_the_existing_receipt_marker_and_real_source_filename() {
    use kah_cli::observer_review::state::{Purpose, Request};
    let request = Request {
        id: "review-marker".into(),
        worker: "worker".into(),
        purpose: Purpose::Checkpoint,
        turn: "t".into(),
        head: None,
        intent: Value::Null,
        trigger: json!({"option1":{"stage":"rules","items":[]}}),
        documents: vec![],
        created_ms: 0,
    };
    let text = kah_cli::observer_review::option1::prompt::compose(
        &request,
        std::path::Path::new("/inputs"),
        std::path::Path::new("/outbox/report.json"),
        "bin/kah observe report",
    )
    .unwrap();
    // Existing delivery.rs::prompt_marker binds the exact native receipt.
    assert!(text.starts_with("kah review request review-marker "));
    assert!(text.contains("/inputs/option1.txt"));
    assert!(!text.contains("option1.json"));
}
