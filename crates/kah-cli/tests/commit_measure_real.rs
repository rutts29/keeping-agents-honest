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
fn real_no_caps_regressions_are_prepared_without_a_component_link_or_any_client() {
    let labels = labels();
    for case in labels["positives"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["requirement"] == "no_caps")
    {
        let mut bundle =
            corpus::real_commit(&repo(), case["commit"].as_str().unwrap(), &labels).unwrap();
        bundle.input.impacted.clear();
        let plan = commit_measure::prepare(&bundle).unwrap();
        assert!(plan.touched.contains("no_caps"));
        assert_eq!(plan.literal.questions.len(), 1);
        let body = plan.literal.body().unwrap();
        assert!(body.contains("Do not impose input limits without explicit approval."));
        assert!(body.contains("current_permissions"));
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
        // The normal test-root cleanup owns this temporary fixture; no source/evidence deleted here.
    }
}
#[test]
fn actual_added_cap_quote_is_verified_and_safe_choice_order_is_preserved() {
    let mut bundle = corpus::real_commit(&repo(), "db06595", &labels()).unwrap();
    let path = "crates/kah-cli/src/in_turn.rs";
    let file = &bundle.input.files[path];
    let (index, text) = file
        .text
        .split_inclusive('\n')
        .enumerate()
        .find(|(_, line)| line.contains("let mut end = text.len().min(4096);"))
        .unwrap();
    bundle.input.matrix = vec![Row {
        requirement: "no_caps".into(),
        finding: "The tool event text is limited to 4096 bytes.".into(),
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
    assert_eq!(plan.absence_rows, ["no_caps".to_owned()].into());
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
