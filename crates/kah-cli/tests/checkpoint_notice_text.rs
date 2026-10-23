//! The worker notice of a failed checkpoint: one entry per
//! finding text, no host-only rows, and plain labels for user requirement ids.
use kah_cli::observer_review::option1::consume::notice_findings;
use kah_observer::commit_check::{FindingKind, Row, Status};

const USER: &str = "user-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const MPS: &str = "The example accelerator result cannot be checked in this environment.";
const STOP: &str = "The example parser stores an optional upper bound.";

fn row(requirement: &str, finding: &str, status: Status) -> Row {
    Row {
        requirement: requirement.into(),
        finding: finding.into(),
        status,
        kind: FindingKind::Behavior,
        quotes: vec![],
    }
}

#[test]
fn a_finding_shared_by_several_requirements_is_shown_once() {
    // Several requirements share the same finding.
    let rows = [
        row("check-example-accelerator", MPS, Status::Unclear),
        row("fix-example-parser", STOP, Status::Violated),
        row("goal", STOP, Status::Violated),
        row("r-tests", "Tests pass.", Status::Satisfied),
        row("example-parser-parity", STOP, Status::Violated),
        row(USER, STOP, Status::Violated),
    ];
    assert_eq!(
        notice_findings(&rows).unwrap(),
        format!(
            "check-example-accelerator: {MPS}; fix-example-parser, goal, example-parser-parity, your request: {STOP}"
        )
    );
}

#[test]
fn the_host_intent_row_is_not_shown_to_the_worker() {
    let pending = row(
        "controller_intent_pending",
        "Not checkable here: no accepted user intent exists yet.",
        Status::Unclear,
    );
    assert_eq!(notice_findings(std::slice::from_ref(&pending)), None);
    let rows = [pending, row("fix-example-parser", STOP, Status::Violated)];
    assert_eq!(
        notice_findings(&rows).unwrap(),
        format!("fix-example-parser: {STOP}")
    );
}

#[test]
fn only_a_complete_user_item_id_becomes_a_plain_label() {
    let rows = [
        row(USER, MPS, Status::Unclear),
        row("user-notes", STOP, Status::Violated),
    ];
    assert_eq!(
        notice_findings(&rows).unwrap(),
        format!("your request: {MPS}; user-notes: {STOP}")
    );
}
