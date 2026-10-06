use super::super::super::state::Purpose;
use super::*;
use serde_json::json;

#[test]
fn host_reproduces_exact_current_report_rejection_and_rejects_stale_or_forged_records() {
    let root = std::env::temp_dir().join(format!("kah-rejected-report-{}", std::process::id()));
    let outbox = root.join("outbox");
    fs::create_dir_all(&outbox).unwrap();
    let report = outbox.join("review-test.json");
    let bytes = b"{\"request_id\":\"review-test\",\"unknown\":true}";
    fs::write(&report, bytes).unwrap();
    let request = Request {
        id: "review-test".into(),
        worker: "disposable-worker".into(),
        purpose: Purpose::Drift,
        turn: "t".into(),
        head: None,
        intent: json!({}),
        trigger: json!({}),
        documents: vec![],
        created_ms: 0,
    };
    let error = workspace::validate_report_bytes(&request, bytes)
        .err()
        .unwrap();
    let actual = submit(&root, &request, &report).unwrap_err();
    assert_eq!(actual.to_string(), error.to_string());
    assert!(
        verified(&outbox, &request)
            .unwrap()
            .unwrap()
            .contains("REVIEW_REPORT_JSON")
    );
    let mut other = request.clone();
    other.turn = "other".into();
    assert!(verified(&outbox, &other).unwrap().is_none());
    fs::write(&report, b"different report").unwrap();
    assert!(verified(&outbox, &request).unwrap().is_none());
    fs::write(&report, bytes).unwrap();
    let invented = crate::observer_review::failure::validation::<()>(
        crate::observer_review::failure::Origin::ReportValidation,
        || Err("invented validation failure".into()),
    )
    .unwrap_err();
    record(&outbox, &request, &report, bytes, invented.as_ref()).unwrap();
    assert!(verified(&outbox, &request).unwrap().is_none());
    record(&outbox, &request, &report, bytes, error.as_ref()).unwrap();
    fs::write(outbox.join("submitted-review-test.json"), "{}").unwrap();
    assert!(
        verified(&outbox, &request).unwrap().is_some(),
        "forged ACK is not evidence"
    );
    fs::remove_dir_all(root).unwrap();
}

fn submit(root: &Path, request: &Request, report: &Path) -> Result<()> {
    let input = root.join("inputs/review-test");
    fs::create_dir_all(&input)?;
    let request_path = input.join("request.json");
    fs::write(&request_path, serde_json::to_vec(request)?)?;
    workspace::submit(&[
        "observe".into(),
        "report".into(),
        "--request".into(),
        request_path.to_str().unwrap().into(),
        "--json".into(),
        report.to_str().unwrap().into(),
    ])
}
