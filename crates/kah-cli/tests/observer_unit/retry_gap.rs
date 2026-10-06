use super::*;

#[test]
fn native_activity_clears_delivery_noise_but_preserves_report_validation() {
    let worker = format!("retry-validation-{}", std::process::id());
    let store = Store::open(&worker).unwrap();
    let path = store.directory.join("gap.json");
    for (error, retained) in [
        ("REVIEW_PHASE_CLAIM_UNCERTAIN stage=checkpoint", true),
        ("REVIEW_PROTOCOL_FACT_CONTRADICTION stage=report", true),
        ("REVIEW_REPORT_MISSING stage=observer", true),
        ("REVIEW_SUBMISSION_UNCONFIRMED stage=delivery", false),
        ("REVIEW_NOT_IDLE stage=delivery", false),
        ("REVIEW_OBSERVER_STALLED stage=liveness", false),
    ] {
        crate::unattended_report::save(
            &store.directory,
            "gap.json",
            &serde_json::json!({"error":error}),
        )
        .unwrap();
        clear_confirmed_delivery_gap(&path).unwrap();
        assert_eq!(path.exists(), retained, "{error}");
    }
    std::fs::remove_dir_all(store.directory.parent().unwrap()).unwrap();
}
