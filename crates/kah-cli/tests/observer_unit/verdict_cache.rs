use super::*;
use serde_json::json;

fn evidence() -> Evidence {
    Evidence {
        authority: [("goal".into(), "goal-digest".into())].into(),
        blobs: [("code".into(), "blob-digest".into())].into(),
        plans: [("plan".into(), "plan-digest".into())].into(),
        components: [("parser".into(), "component-digest".into())].into(),
        tests: [("parser-test".into(), "result-digest".into())].into(),
    }
}
fn entry() -> Entry {
    Entry {
        requirement: "parser".into(),
        review: "review-original".into(),
        head: "a".repeat(40),
        evidence: evidence(),
        confirmed: true,
        always: false,
        verdict: json!({"state":"satisfied"}),
    }
}
#[test]
fn identical_evidence_carries_visible_lineage_but_gates_and_always_items_rejudge() {
    let entry = entry();
    let carried = entry.carry(&evidence(), false).unwrap();
    assert_eq!(carried["carried_from_review"], "review-original");
    assert_eq!(carried["carried_from_sha"], "a".repeat(40));
    assert_eq!(carried["verdict"]["state"], "satisfied");
    assert!(entry.carry(&evidence(), true).is_none());
    let mut always = entry.clone();
    always.always = true;
    assert!(always.carry(&evidence(), false).is_none());
    always.always = false;
    always.confirmed = false;
    assert!(always.carry(&evidence(), false).is_none());
}
#[test]
fn each_evidence_dependency_invalidates_independently() {
    let entry = entry();
    for field in ["authority", "blobs", "plans", "components", "tests"] {
        let mut value = serde_json::to_value(evidence()).unwrap();
        let first = value[field]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap()
            .clone();
        value[field][first] = json!("changed");
        let changed: Evidence = serde_json::from_value(value).unwrap();
        assert!(entry.carry(&changed, false).is_none(), "{field}");
    }
}
#[test]
fn protected_categories_are_typed_not_name_heuristics() {
    use kah_observer::decision_units::Dimension;
    for dimension in [Dimension::Drift, Dimension::Steering] {
        assert!(always(dimension, false, false, false));
    }
    assert!(always(Dimension::Intent, true, false, false));
    assert!(always(Dimension::Intent, false, true, false));
    assert!(always(Dimension::Intent, false, false, true));
    assert!(!always(Dimension::Integration, false, false, false));
}

fn request_and_requirement() -> (
    crate::observer_review::state::Request,
    kah_observer::decision_units::Requirement,
) {
    use crate::observer_review::state::{Purpose, Request};
    use kah_observer::review_evidence::Document;
    let transcript = Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "First whole record.\nSecond whole record.\n",
    );
    let requirement = serde_json::from_value(json!({"id":"parser","name":"Parser works","dimension":"integration",
        "authority_items":["goal"],"citations":[
            {"source":"transcript","digest":transcript.digest,"quote":"First whole record."},
            {"source":"transcript","digest":transcript.digest,"quote":"Second whole record."}],
        "components":[],"checks":[],"applicability":{"due":true,"unconditional":false,"reasoning":"Native result at event.","evidence_citations":[0]},"judgment":{"kind":"semantic","decision":"satisfied","reasoning":"Native result.","evidence_citations":[0,1]}})).unwrap();
    let request = Request {
        id: "review-test".into(),
        worker: "worker-test".into(),
        purpose: Purpose::Checkpoint,
        turn: "turn".into(),
        head: Some("a".repeat(40)),
        intent: json!({"goal":"Implement parser","items":[]}),
        trigger: json!({"review_class":"commit"}),
        documents: vec![transcript],
        created_ms: 1,
    };
    (request, requirement)
}
#[test]
fn native_record_growth_does_not_erase_either_cited_record_fingerprint() {
    let (mut request, requirement) = request_and_requirement();
    let before = capture::fingerprint(&request, &requirement).unwrap();
    assert_eq!(before.blobs.len(), 2);
    request.documents[0] = kah_observer::review_evidence::Document::new(
        "transcript",
        "worker_transcript",
        "native:worker",
        "First whole record.\nSecond whole record.\nUnrelated later record.\n",
    );
    assert_eq!(
        before,
        capture::fingerprint(&request, &requirement).unwrap()
    );
    request.intent["goal"] = json!("Different user request");
    assert_ne!(
        before,
        capture::fingerprint(&request, &requirement).unwrap()
    );
}
#[test]
fn completed_light_review_records_private_cache_without_changing_gate_behavior() {
    use kah_observer::{
        judge::{Judgment, Metrics},
        review_evidence::Report,
    };
    let (mut request, requirement) = request_and_requirement();
    request.worker = format!("cache-capture-{}", crate::relay_store::nonce());
    let store = crate::observer_review::state::Store::open(&request.worker).unwrap();
    let report: Report = serde_json::from_value(json!({"request_id":request.id,"findings":[],
        "decision_units":{"version":12,"requirements":[requirement]}}))
    .unwrap();
    let judgment = Judgment {
        backend: "jev".into(),
        model: "jev".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    };
    record(&store, &request, &report, &judgment).unwrap();
    let cache = store.directory.join("verdict-cache");
    let path = std::fs::read_dir(&cache)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let saved: Entry = crate::observer_review::state::read(&path).unwrap().unwrap();
    assert_eq!(saved.review, request.id);
    assert!(
        !saved.confirmed,
        "missing Jev answer must never become a carried verdict"
    );
    record(&store, &request, &report, &supported(&judgment)).unwrap();
    let without_check: Entry = crate::observer_review::state::read(&path).unwrap().unwrap();
    assert!(
        !without_check.confirmed,
        "semantic support without executable check remains dirty"
    );
    request.trigger = json!({"review_class":"gate"});
    request.id = "review-gate".into();
    record(&store, &request, &report, &judgment).unwrap();
    let unchanged: Entry = crate::observer_review::state::read(&path).unwrap().unwrap();
    assert_eq!(unchanged.review, saved.review);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
fn supported(original: &kah_observer::judge::Judgment) -> kah_observer::judge::Judgment {
    let mut judgment = original.clone();
    let answer = kah_observer::judge::Answer::Choice {
        choice: "supported".into(),
        confidence: 0.99,
        probabilities: [
            ("supported".into(), 0.99),
            ("contradicted".into(), 0.005),
            ("unclear".into(), 0.005),
        ]
        .into(),
    };
    for key in ["requirement.parser", "applicability.parser"] {
        judgment.answers.insert(key.into(), answer.clone());
    }
    judgment
}

#[test]
fn confirmed_not_due_is_cached_but_unconfirmed_applicability_is_not() {
    use kah_observer::{
        judge::{Judgment, Metrics},
        review_evidence::Report,
    };
    // USER C: every non-always verdict carries until its evidence changes/gate.
    let (mut request, mut requirement) = request_and_requirement();
    request.worker = format!("cache-not-due-{}", crate::relay_store::nonce());
    requirement.applicability.as_mut().unwrap().due = false;
    let store = crate::observer_review::state::Store::open(&request.worker).unwrap();
    let report: Report = serde_json::from_value(json!({"request_id":request.id,"findings":[],
        "decision_units":{"version":12,"requirements":[requirement]}}))
    .unwrap();
    let mut judgment = supported(&Judgment {
        backend: "jev".into(),
        model: "jev".into(),
        answers: Default::default(),
        metrics: Metrics::default(),
        fallback_reason: None,
    });
    record(&store, &request, &report, &judgment).unwrap();
    let path = std::fs::read_dir(store.directory.join("verdict-cache"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let entry: Entry = crate::observer_review::state::read(&path).unwrap().unwrap();
    assert!(
        entry.confirmed,
        "a confirmed not-due verdict is not missing evidence"
    );
    judgment.answers.remove("applicability.parser");
    record(&store, &request, &report, &judgment).unwrap();
    let entry: Entry = crate::observer_review::state::read(&path).unwrap().unwrap();
    assert!(!entry.confirmed);
    let root = store.directory.parent().unwrap().to_owned();
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
