//! The worker notice of a failed checkpoint (finding #44): one entry per
//! finding text, no host-only rows, and plain labels for user requirement ids.
use kah_cli::observer_review::option1::consume::notice_findings;
use kah_observer::commit_check::{FindingKind, Row, Status};

const USER: &str = "user-ab68824e8d6b6d31275a7f8d4b173f797c07c7097b3af6cf7d9a5acd0c111b25";
const MPS: &str = "Not checkable here as an actual current MPS runtime result.";
const STOP: &str = "At 02ab45b, validate_slice_stops extracts the stop into Option<isize>.";

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
    // The shape of the 878-10b notice at 02:26:55 (noise-11 M4).
    let rows = [
        row("check-current-torch-mps", MPS, Status::Unclear),
        row("fix-878", STOP, Status::Violated),
        row("goal", STOP, Status::Violated),
        row("r-tests", "Tests pass.", Status::Satisfied),
        row("slice-parity", STOP, Status::Violated),
        row(USER, STOP, Status::Violated),
    ];
    assert_eq!(
        notice_findings(&rows).unwrap(),
        format!(
            "check-current-torch-mps: {MPS}; fix-878, goal, slice-parity, your request: {STOP}"
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
    let rows = [pending, row("fix-878", STOP, Status::Violated)];
    assert_eq!(notice_findings(&rows).unwrap(), format!("fix-878: {STOP}"));
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
