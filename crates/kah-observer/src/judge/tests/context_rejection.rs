use super::context_rejection;
use serde_json::json;

#[test]
fn provider_context_errors_request_preparation_but_other_validation_errors_do_not() {
    for body in [
        json!({"detail":"State and longest question exceed the maximum context length"}),
        json!({"error":{"message":"input tokens exceed limit"}}),
        json!({"detail":[{"msg":"request context too long"}]}),
    ] {
        assert!(context_rejection(&body));
    }
    assert!(!context_rejection(
        &json!({"error":"Unknown question type"})
    ));
    assert!(!context_rejection(&json!({"detail":"Invalid API token"})));
}
