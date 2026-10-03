use kah_observer::review_evidence::Report;
use serde_json::json;
#[test]
fn concise_native_reminder_keeps_verified_user_words_and_a_full_record_pointer() {
    let intent = json!({"goal":"Finish cumulative integration.","items":[{"id":"user-notes","text":"Do not commit notes/. Keep them local."}]});
    let report:Report=serde_json::from_value(json!({"request_id":"review-1","findings":[],"reminder":{"instruction":"Correct the delivered artifact before continuing.","quotes":[{"id":"user-notes","quote":"Do not commit notes/."}]}})).unwrap();
    let text = report.reminder_text(&intent, "/public/ledger.txt").unwrap();
    assert!(text.contains("user-notes"));
    assert!(text.contains("Do not commit notes/."));
    assert!(text.contains("/public/ledger.txt"));
    let wrong = json!({"goal":"Finish cumulative integration.","items":[{"id":"user-notes","text":"Do commit notes/."}]});
    assert!(
        report
            .reminder_text(&wrong, "/public/ledger.txt")
            .unwrap_err()
            .contains("REVIEW_REMINDER_QUOTE")
    );
}
