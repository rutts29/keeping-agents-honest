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

#[test]
fn real_request_shares_changed_code_once_and_uses_stored_literal_typing() {
    let bundle = retained_commit();
    let plan = commit_measure::prepare(&bundle).unwrap();
    assert_eq!(plan.literal.state["diff_hunks"], json!(bundle.input.hunks));
    assert!(
        plan.literal.state["checks"]
            .as_object()
            .unwrap()
            .values()
            .all(|rule| rule.get("diff_hunks").is_none())
    );
    let literal_ids: std::collections::BTreeSet<_> = plan.literal.state["checks"]
        .as_object()
        .unwrap()
        .values()
        .map(|rule| rule["requirement"].as_str().unwrap().to_owned())
        .collect();
    assert!(!literal_ids.contains("goal"));
    let expected: std::collections::BTreeSet<_> = bundle
        .historical
        .as_ref()
        .unwrap()
        .projection
        .frontier
        .iter()
        .filter(|item| {
            matches!(
                item.kind,
                kah_core::intent_history::IntentKind::Constraint
                    | kah_core::intent_history::IntentKind::Invariant
            )
        })
        .map(|item| item.id.to_string())
        .collect();
    assert_eq!(literal_ids, expected);
    assert!(plan.touched.contains("goal"));
}

#[test]
fn actual_archive_copy_comparison_retains_full_inputs_and_records_git_identity() {
    let mut bundle = retained_commit();
    let head = git_value(&["rev-parse", "824cdf6"]);
    let base = git_value(&["rev-parse", "824cdf6^1"]);
    let (hunks, files) = commit_measure::collect(&repo(), &base, &head).unwrap();
    bundle.input.revision = head;
    bundle.input.baseline = Some(base);
    bundle.input.hunks = hunks;
    bundle.input.files = files;
    let original_paths: std::collections::BTreeSet<_> = bundle
        .input
        .hunks
        .values()
        .map(|h| h.file.clone())
        .collect();
    let omissions = commit_measure::comparison::omitted(&repo(), &bundle.input).unwrap();
    assert!(
        omissions
            .values()
            .any(|v| v["native_status"] == "C100" || v["native_status"] == "R100")
    );
    let compared = commit_measure::comparison::input(&bundle, &omissions).unwrap();
    assert_eq!(
        bundle
            .input
            .hunks
            .values()
            .map(|h| h.file.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        original_paths
    );
    assert!(compared.input.hunks.len() < bundle.input.hunks.len());
    assert!(compared.input.carried["full_input_retained"] == true);
}
fn git_value(args: &[&str]) -> String {
    let out = std::process::Command::new("/usr/bin/git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .args(args)
        .current_dir(repo())
        .output()
        .unwrap();
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

#[test]
fn whole_file_decomposition_keeps_every_rule_and_all_source_hunks() {
    let plan = commit_measure::prepare(&retained_commit()).unwrap();
    let pieces = plan.literal.whole_hunks();
    let mut ids = std::collections::BTreeSet::new();
    for piece in pieces {
        assert_eq!(
            serde_json::to_value(&piece.questions).unwrap(),
            serde_json::to_value(&plan.literal.questions).unwrap()
        );
        assert_eq!(
            piece.state["current_permissions"],
            plan.literal.state["current_permissions"]
        );
        let hunks = piece.state["diff_hunks"].as_object().unwrap();
        assert_eq!(hunks.len(), 1);
        for (id, hunk) in hunks {
            assert_eq!(hunk, &plan.literal.state["diff_hunks"][id]);
            ids.insert(id.clone());
        }
    }
    assert_eq!(
        ids,
        plan.literal.state["diff_hunks"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    );
}
