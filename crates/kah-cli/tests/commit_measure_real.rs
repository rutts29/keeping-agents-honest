//! Retained real F6/F18 bugs: unconditional no-caps must not need a map link.
//! Offline regression for preserving constraints in complete request bodies.
use kah_cli::commit_measure::{self, corpus};
use kah_observer::commit_check::{CodeQuote, FindingKind, Row, Status};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn labels() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/observer-phase1/labels-oct2-8.json"
    ))
    .unwrap()
}
#[test]
fn historical_constraints_are_prepared_without_a_component_link_or_any_client() {
    let mut bundle = retained_commit();
    let expected = bundle
        .input
        .rules
        .iter()
        .filter(|(_, r)| r.unconditional_prohibition)
        .map(|(id, _)| id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!expected.is_empty());
    bundle.input.impacted.clear();
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert_eq!(plan.touched, expected);
    let body = plan.literal.body().unwrap();
    assert!(body.contains("current_permissions"));
    assert!(!body.contains("jevliteralquestions"));
    assert!(bundle.quality.is_empty());
    let directory =
        std::env::temp_dir().join(format!("phase1-measure-{}", kah_cli::relay_store::nonce()));
    let report = commit_measure::count(&bundle, &directory).unwrap();
    assert_eq!(report["actual_jev_calls"], 0);
    assert_eq!(report["actual_jev_cost_usd"], 0.0);
    assert_eq!(report["quality_judgment"], "not_run");
    assert_eq!(
        fs::read_to_string(directory.join("literal.json")).unwrap(),
        body
    );
}
#[test]
fn missing_historical_intent_is_reported_instead_of_invented_from_later_labels() {
    let error = corpus::real_commit(&repo(), "db06595").err().unwrap();
    assert!(
        error
            .to_string()
            .starts_with("MEASURE_HISTORICAL_INTENT_MISSING")
    );
}
#[test]
fn actual_added_cap_quote_is_verified_and_safe_choice_order_is_preserved() {
    let mut bundle = retained_commit();
    let path = "crates/kah-cli/src/observer_review/consume.rs";
    let rule_id = bundle
        .input
        .intent
        .items
        .iter()
        .find(|(_, a)| a.text.contains("Remove the unapproved input limits."))
        .unwrap()
        .0
        .clone();
    bundle.input.rules.get_mut(&rule_id).unwrap().prohibition = true;
    let file = &bundle.input.files[path];
    let (index, text) = file
        .text
        .split_inclusive('\n')
        .enumerate()
        .find(|(_, line)| line.contains("Duration::from_secs(110)"))
        .unwrap();
    bundle.input.matrix = vec![Row {
        requirement: rule_id.clone(),
        finding: "The consume path sets a 110 second completion deadline.".into(),
        status: Status::Violated,
        kind: FindingKind::Behavior,
        quotes: vec![CodeQuote {
            file: path.into(),
            first_line: index + 1,
            last_line: index + 1,
            text: text.into(),
            digest: file.digest.clone(),
        }],
    }];
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert!(plan.correction_signals.is_empty());
    assert_eq!(plan.citations.questions.len(), 1);
    let body = plan.citations.body().unwrap();
    assert!(body.find("\"says_nothing\":").unwrap() < body.find("\"contradicts\":").unwrap());
    assert!(body.find("\"contradicts\":").unwrap() < body.find("\"supports\":").unwrap());
    let mut absence = bundle.input.matrix[0].clone();
    absence.status = Status::Satisfied;
    absence.kind = FindingKind::NotViolated;
    absence.finding = "No prohibited cap added.".into();
    absence.quotes.clear();
    bundle.input.matrix = vec![absence];
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert!(plan.citations.questions.is_empty());
    assert_eq!(plan.absence_rows, [rule_id].into());
}
#[test]
fn labelled_control_sample_is_deterministic_and_excludes_every_positive_and_fix() {
    let labels = labels();
    let first = corpus::controls(&repo(), &labels).unwrap();
    assert_eq!(first, corpus::controls(&repo(), &labels).unwrap());
    assert!(first.len() >= labels["positives"].as_array().unwrap().len() * 4);
    for control in &first {
        for positive in labels["positives"].as_array().unwrap() {
            assert!(!control.starts_with(positive["commit"].as_str().unwrap()));
            assert!(!control.starts_with(positive["fixed_by"].as_str().unwrap()));
        }
    }
    assert_eq!(json!(first).as_array().unwrap().len(), 20);
}

#[test]
fn scoring_labels_cannot_change_the_same_commits_authority_or_questions() {
    let original = labels();
    let mut changed = original.clone();
    changed["requirements"]["no_caps"]["text"] = json!("Injected scoring-only instruction");
    changed["requirements"]["jev_literal_questions"]["text"] = json!("Another injected label");
    let from_scoring_case = |scoring: &Value| {
        let sha = scoring["positives"][3]["commit"].as_str().unwrap();
        {
            assert_eq!(sha, "bfd37e2");
            retained_commit()
        }
    };
    let before = from_scoring_case(&original);
    let after = from_scoring_case(&changed);
    assert!(before.quality.is_empty());
    assert!(!before.input.rules.contains_key("no_caps"));
    assert!(!before.input.rules.contains_key("jev_literal_questions"));
    assert_eq!(
        commit_measure::prepare(&before)
            .unwrap()
            .literal
            .body()
            .unwrap(),
        commit_measure::prepare(&after)
            .unwrap()
            .literal
            .body()
            .unwrap()
    );
}

fn retained_commit() -> commit_measure::Bundle {
    // Actual digest-verified historical snapshot from the construction replay;
    // full-suite tests intentionally isolate KAH_STATE_DIR and native HOME.
    let snapshot = serde_json::from_str(include_str!(
        "../../../docs/gates/evidence/observer-phase1/historical-bfd37e2.json"
    ))
    .unwrap();
    corpus::real_commit_with_snapshot(&repo(), "bfd37e2", snapshot).unwrap()
}

#[test]
fn scoring_matches_labelled_code_evidence_not_injected_authority_aliases() {
    let label = labels()["positives"][3].clone();
    let bundle = retained_commit();
    let rule_id = bundle
        .input
        .rules
        .keys()
        .find(|id| id.starts_with("user-"))
        .unwrap();
    let source = bundle.input.files["crates/kah-cli/src/observer_review/consume.rs"]
        .text
        .lines()
        .find(|line| line.contains("Duration::from_secs(110)"))
        .unwrap();
    let case = json!({"id":"real-bfd37e2","kind":"real_positive","label":label,
        "matrix_rows":[{"requirement":rule_id,"status":"violated",
        "quotes":[{"file":"crates/kah-cli/src/observer_review/consume.rs","text":source}]}]});
    let score = commit_measure::replay::summarize(std::slice::from_ref(&case));
    assert_eq!(score["matrix_misses"], 0);
    let mut wrong = case;
    wrong["matrix_rows"][0]["quotes"][0]["text"] = json!("An unrelated line");
    assert_eq!(
        commit_measure::replay::summarize(&[wrong])["matrix_misses"],
        1
    );
}
