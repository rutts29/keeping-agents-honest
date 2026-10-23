//! Policy discriminators from official Codex tag and installed Claude2.1.296.
use kah_cli::observer_review::{
    failure::{Origin, correctable},
    provider_block, redelivery,
};
use serde_json::json;
use std::fs;
#[test]
fn native_policy_classes_block_but_prose_rate_limits_and_tool_errors_do_not() {
    let root =
        std::env::temp_dir().join(format!("provider-types-{}", kah_cli::relay_store::nonce()));
    fs::create_dir(&root).unwrap();
    let path = root.join("records.jsonl");
    // Claude Code 2.1.296 transcript fields: refusal stop reason and DLP/safety errors.
    for row in [
        json!({"type":"assistant","apiError":"dlp_request_denied"}),
        json!({"type":"assistant","apiError":"safety_monitor_blocked"}),
        json!({"type":"assistant","message":{"stop_reason":"refusal"}}),
    ] {
        fs::write(&path, format!("{row}\n")).unwrap();
        let block = provider_block::detect("claude", &path).unwrap().unwrap();
        assert_eq!(block.offset, 0);
        assert_eq!(block.end, fs::metadata(&path).unwrap().len());
    }
    for row in [
        json!({"type":"assistant","error":"rate_limit","apiError":"usage_limit_reached"}),
        json!({"type":"assistant","error":"server_error","message":{"content":"Request timed out"}}),
        json!({"type":"user","apiError":"safety_monitor_blocked"}),
        json!({"type":"assistant","isSidechain":true,"apiError":"safety_monitor_blocked"}),
        json!({"type":"user","message":{"content":"misalignment_policy_violation"}}),
    ] {
        fs::write(&path, format!("{row}\n")).unwrap();
        assert!(provider_block::detect("claude", &path).unwrap().is_none());
        assert!(provider_block::detect("codex", &path).unwrap().is_none());
    }
    let error = "REVIEW_PROVIDER_BLOCKED stage=native_provider: human decision required";
    for origin in [
        Origin::Host,
        Origin::Delivery,
        Origin::ReportValidation,
        Origin::SelectionValidation,
    ] {
        assert!(!correctable(origin, error));
        assert!(!redelivery::retryable(origin, error));
    }
    fs::remove_dir_all(root).unwrap();
}
