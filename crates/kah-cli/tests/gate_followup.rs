use kah_cli::observer_review::{
    gate_approval::{self, followup::*},
    source_store,
    state::{Purpose, Request},
};
use kah_observer::review_evidence::Report;
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture {
    root: PathBuf,
    request: Request,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "kah-followup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        kah_core::run_store::ensure_private_directory(&root.join("requests")).unwrap();
        kah_core::run_store::ensure_private_directory(&root.join("sources")).unwrap();
        let request = Request {
            id: "review-original".into(),
            worker: "worker".into(),
            purpose: Purpose::Checkpoint,
            turn: "turn".into(),
            head: Some("a".repeat(40)),
            intent: json!({"revision":1,"goal":"Deliver integrated gate"}),
            trigger: json!({}),
            documents: vec![],
            created_ms: 0,
        };
        let fixture = Self { root, request };
        fixture.archive(&fixture.request);
        fixture
    }
    fn archive(&self, request: &Request) {
        source_store::save(
            &self.root.join("requests"),
            &format!("{}.json", request.id),
            &serde_json::to_value(request).unwrap(),
        )
        .unwrap();
    }
    fn finish(&self) -> VerificationRound {
        let round = start(&self.root, &self.request).unwrap();
        advance(
            &self.root,
            &round.binding,
            Step::Evidence(vec![pointer("existing")]),
        )
        .unwrap();
        advance(&self.root, &round.binding, Step::Builder(builder(&round))).unwrap();
        advance(&self.root, &round.binding, Step::Checks(checks(&round))).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn pointer(receipt: &str) -> EvidencePointer {
    EvidencePointer {
        receipt: receipt.into(),
        record: format!("private:/{receipt}.json"),
        digest: "d".repeat(64),
    }
}
fn builder(round: &VerificationRound) -> BuilderReceipt {
    BuilderReceipt {
        request: pointer(&round.builder_receipt()),
        response: BuilderOutcome::Evidence {
            response: pointer("builder-answer"),
        },
    }
}
fn proof() -> gate_approval::DeliveryProof {
    serde_json::from_value(json!({"head":"a".repeat(40),"fresh_checkout":true,
        "tests":[{"command":"integration-check","commit":"a".repeat(40),"exit_code":0,"success":true}],
        "artifacts":[{"path":"PLAN.md","baseline_blob":"b".repeat(40),"delivered_blob":"b".repeat(40),"delivered_sha":"a".repeat(40)}]})).unwrap()
}
fn checks(round: &VerificationRound) -> CheckReceipt {
    CheckReceipt { verified:vec![], receipt:pointer(&round.checks_receipt()), proof:proof(), risks:vec![UnverifiedRisk {
        requirement_id:"integration".into(), unverified:"Cumulative behavior across providers could not be confirmed.".into(),
        tried:vec![Attempt { what:"Read existing receipts, asked builder for integration evidence, reran same-SHA checks.".into(), receipt:round.checks_receipt() }],
        acceptance_risk:"Accepting may advance with an unproven provider interaction.".into(), evidence:vec![pointer("existing"),pointer("builder-answer")],
    }] }
}
fn report(request: &Request) -> Report {
    serde_json::from_value(json!({"request_id":request.id,"findings":[],"decision_units":{"version":8,"requirements":[{
        "id":"integration","name":"Cumulative integration","dimension":"integration","authority_items":["goal"],"citations":[],"components":[],"checks":[],
        "judgment":{"kind":"semantic","decision":"satisfied","reasoning":"The demonstrated behavior satisfies the requirement.","evidence_citations":[0]}
    }]}})).unwrap()
}
fn assessment() -> Value {
    json!({"passed":null,"violations":[],"needs_user":[],"needs_thinker":["integration"]})
}

#[test]
fn absent_or_incomplete_followup_never_admits_user_question() {
    let f = Fixture::new();
    assert!(!gate_approval::eligible(
        &f.request,
        &report(&f.request),
        &assessment(),
        &proof()
    ));
    assert!(completed(&f.root, &f.request).unwrap().is_none());
    assert!(
        gate_approval::store(
            &f.root,
            &f.request,
            &report(&f.request),
            &assessment(),
            &proof(),
            &json!({})
        )
        .unwrap()
        .is_none()
    );
    let round = start(&f.root, &f.request).unwrap();
    assert!(advance(&f.root, &round.binding, Step::Checks(checks(&round))).is_err());
    assert!(completed(&f.root, &f.request).unwrap().is_none());
    assert!(!f.root.join("questions").exists());
}

#[test]
fn once_only_stage_claims_and_receipts_survive_repeated_events() {
    let f = Fixture::new();
    let round = start(&f.root, &f.request).unwrap();
    let step = Step::Evidence(vec![pointer("existing")]);
    let next = advance(&f.root, &round.binding, step.clone()).unwrap();
    assert_eq!(next.stage, Stage::Builder);
    assert!(claim(&f.root, &round.binding, Stage::Builder).unwrap());
    assert!(!claim(&f.root, &round.binding, Stage::Builder).unwrap());
    assert_eq!(start(&f.root, &f.request).unwrap().id, round.id);
    assert_eq!(
        advance(&f.root, &round.binding, step).unwrap().stage,
        Stage::Builder
    );
    assert!(
        advance(
            &f.root,
            &round.binding,
            Step::Evidence(vec![pointer("replacement")])
        )
        .is_err()
    );
    advance(&f.root, &round.binding, Step::Builder(builder(&round))).unwrap();
    assert!(claim(&f.root, &round.binding, Stage::Checks).unwrap());
    assert!(!claim(&f.root, &round.binding, Stage::Checks).unwrap());
    let finished = advance(&f.root, &round.binding, Step::Checks(checks(&round))).unwrap();
    assert_eq!(finished.stage, Stage::Complete);
    assert_eq!(
        advance(&f.root, &round.binding, Step::Checks(checks(&round)))
            .unwrap()
            .id,
        round.id
    );
    assert!(!claim(&f.root, &round.binding, Stage::Checks).unwrap());
}

#[test]
fn complete_positive_round_admits_one_plain_risk_decision() {
    let f = Fixture::new();
    f.finish();
    let done = completed(&f.root, &f.request).unwrap().unwrap();
    assert!(gate_approval::eligible_after_followup(
        &f.request,
        &report(&f.request),
        &assessment(),
        &proof(),
        &done
    ));
    let q = gate_approval::store(
        &f.root,
        &f.request,
        &report(&f.request),
        &assessment(),
        &proof(),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    let repeat = gate_approval::store(
        &f.root,
        &f.request,
        &report(&f.request),
        &assessment(),
        &proof(),
        &json!({}),
    )
    .unwrap()
    .unwrap();
    assert_eq!(q["id"], repeat["id"]);
    let text = gate_approval::message(&q).unwrap();
    assert!(text.contains("Accept gate"));
    assert!(text.contains("or send it back for more evidence?"));
    assert!(!text.contains("Jev answers:"));
    assert!(!text.contains("Thinker\'s conclusions/reasoning/sources:"));
    assert!(text.contains("Full record for question-"));
    assert!(text.contains("Cumulative behavior across providers could not be confirmed."));
    assert!(text.contains("reran same-SHA checks"));
    assert!(text.contains("Accepting may advance with an unproven provider interaction."));
    assert!(!text.contains("Approve or reject?"));
}

#[test]
fn violations_status_observations_and_nonpositive_semantics_never_ask_user() {
    let f = Fixture::new();
    f.finish();
    let done = completed(&f.root, &f.request).unwrap().unwrap();
    let mut bad = assessment();
    bad["violations"] = json!(["integration"]);
    assert!(!gate_approval::eligible_after_followup(
        &f.request,
        &report(&f.request),
        &bad,
        &proof(),
        &done
    ));
    let mut status = assessment();
    status["state"] = "checkpoint_unconfirmed_observation".into();
    assert!(!gate_approval::eligible_after_followup(
        &f.request,
        &report(&f.request),
        &status,
        &proof(),
        &done
    ));
    let mut negative = serde_json::to_value(report(&f.request)).unwrap();
    negative["decision_units"]["requirements"][0]["judgment"]["decision"] = "violated".into();
    assert!(!gate_approval::eligible_after_followup(
        &f.request,
        &serde_json::from_value(negative).unwrap(),
        &assessment(),
        &proof(),
        &done
    ));
}

#[test]
fn descendants_reuse_origin_only_through_exact_private_lineage() {
    let f = Fixture::new();
    let round = f.finish();
    let mut child = f.request.clone();
    child.id = "review-child".into();
    child.trigger = json!({"reconsideration":{"request":f.request.id}});
    f.archive(&child);
    assert_eq!(start(&f.root, &child).unwrap().id, round.id);
    let done = completed(&f.root, &child).unwrap().unwrap();
    assert!(gate_approval::eligible_after_followup(
        &child,
        &report(&child),
        &assessment(),
        &proof(),
        &done
    ));
    assert!(!gate_approval::eligible_after_followup(
        &f.request,
        &report(&f.request),
        &assessment(),
        &proof(),
        &done
    ));
    let mut grandchild = child.clone();
    grandchild.id = "review-grandchild".into();
    grandchild.trigger = json!({"reconsideration":{"request":child.id}});
    f.archive(&grandchild);
    assert_eq!(start(&f.root, &grandchild).unwrap().id, round.id);
    let mut stale = child.clone();
    stale.intent["revision"] = 2.into();
    assert!(
        completed(&f.root, &stale).is_err(),
        "caller cannot fabricate request fields"
    );
    f.archive(&stale);
    assert!(
        completed(&f.root, &stale).is_err(),
        "archived parent scope must still match"
    );
    let mut forged = f.request.clone();
    forged.trigger = json!({"followup":{"complete":true}});
    assert!(!gate_approval::eligible(
        &forged,
        &report(&forged),
        &assessment(),
        &proof()
    ));
}

#[test]
fn failed_checks_and_unlinked_or_unnamed_risks_cannot_complete_round() {
    let f = Fixture::new();
    let round = start(&f.root, &f.request).unwrap();
    advance(
        &f.root,
        &round.binding,
        Step::Evidence(vec![pointer("existing")]),
    )
    .unwrap();
    advance(&f.root, &round.binding, Step::Builder(builder(&round))).unwrap();
    let mut bad = checks(&round);
    bad.proof.tests[0].commit = "c".repeat(40);
    assert!(advance(&f.root, &round.binding, Step::Checks(bad)).is_err());
    let mut bad = checks(&round);
    bad.risks[0].acceptance_risk.clear();
    assert!(advance(&f.root, &round.binding, Step::Checks(bad)).is_err());
    let mut bad = checks(&round);
    bad.risks[0].tried[0].receipt = "invented".into();
    assert!(advance(&f.root, &round.binding, Step::Checks(bad)).is_err());
    let mut bad = checks(&round);
    bad.risks[0].evidence[0].digest = "e".repeat(64);
    assert!(advance(&f.root, &round.binding, Step::Checks(bad)).is_err());
    assert!(completed(&f.root, &f.request).unwrap().is_none());
    assert_eq!(
        read(&f.root, &round.binding).unwrap().unwrap().stage,
        Stage::Checks
    );
}

#[test]
fn unavailable_builder_is_an_honest_attempt_and_private_completion_is_required() {
    let f = Fixture::new();
    let round = start(&f.root, &f.request).unwrap();
    advance(
        &f.root,
        &round.binding,
        Step::Evidence(vec![pointer("existing")]),
    )
    .unwrap();
    let mut attempt = builder(&round);
    attempt.response = BuilderOutcome::Unavailable {
        reason: "Recorded delivery rejected because receiver identity changed.".into(),
    };
    advance(&f.root, &round.binding, Step::Builder(attempt)).unwrap();
    let mut checked = checks(&round);
    checked.risks[0].evidence = vec![pointer("existing")];
    checked.risks[0].tried[0] = Attempt {
        what: "Asked builder; recorded receiver mismatch prevented its response.".into(),
        receipt: round.builder_receipt(),
    };
    advance(&f.root, &round.binding, Step::Checks(checked)).unwrap();
    assert!(completed(&f.root, &f.request).unwrap().is_some());
    let path = f
        .root
        .join("gate-followups")
        .join(format!("{}.json", round.id));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        completed(&f.root, &f.request)
            .err()
            .unwrap()
            .to_string()
            .starts_with("REVIEW_FOLLOWUP_PRIVATE")
    );
}

#[test]
fn duplicated_uncertainty_ids_cannot_admit_an_unrelated_risk() {
    let f = Fixture::new();
    let round = start(&f.root, &f.request).unwrap();
    advance(
        &f.root,
        &round.binding,
        Step::Evidence(vec![pointer("existing")]),
    )
    .unwrap();
    advance(&f.root, &round.binding, Step::Builder(builder(&round))).unwrap();
    let mut checked = checks(&round);
    let mut unrelated = checked.risks[0].clone();
    unrelated.requirement_id = "unrelated".into();
    checked.risks.push(unrelated);
    advance(&f.root, &round.binding, Step::Checks(checked)).unwrap();
    let done = completed(&f.root, &f.request).unwrap().unwrap();
    let mut duplicate = assessment();
    duplicate["needs_thinker"] = json!(["integration", "integration"]);
    assert!(!gate_approval::eligible_after_followup(
        &f.request,
        &report(&f.request),
        &duplicate,
        &proof(),
        &done
    ));
}

#[test]
fn awaiting_thinker_and_completed_rounds_are_reused_for_the_same_scope() {
    let f = Fixture::new();
    let first = start(&f.root, &f.request).unwrap();
    advance(
        &f.root,
        &first.binding,
        Step::Evidence(vec![pointer("existing")]),
    )
    .unwrap();
    let awaiting = advance(&f.root, &first.binding, Step::Builder(builder(&first))).unwrap();
    let path = f.root.join("gate-followups").join(&first.id);
    kah_core::run_store::ensure_private_directory(&path).unwrap();
    source_store::save(
        &path,
        "finished.json",
        &json!({"status":"awaiting_thinker"}),
    )
    .unwrap();
    let mut later = f.request.clone();
    later.id = "review-later-claim".into();
    f.archive(&later);
    assert_eq!(start(&f.root, &later).unwrap().id, first.id);
    assert_eq!(existing(&f.root, &later).unwrap().unwrap().id, first.id);
    advance(&f.root, &first.binding, Step::Checks(checks(&awaiting))).unwrap();
    assert_eq!(
        existing(&f.root, &later).unwrap().unwrap().stage,
        Stage::Complete
    );
    later.head = Some("b".repeat(40));
    assert!(existing(&f.root, &later).unwrap().is_none());
    later.head = f.request.head.clone();
    later.intent["revision"] = json!(2);
    assert!(existing(&f.root, &later).unwrap().is_none());
}
