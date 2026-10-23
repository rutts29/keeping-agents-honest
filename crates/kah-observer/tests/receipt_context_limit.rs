//! A provider rejection of an oversized body is JEV_CONTEXT_LIMIT, which
//! routes to whole-item preparation; other errors stay HTTP status errors.
use kah_observer::judge::batch::{RawResponse, Receipt, Tokens, WireRequest};

fn receipt(status: u16, body: &str) -> Receipt {
    Receipt {
        request: WireRequest {
            id: "error-classification-only".into(),
            body: r#"{"questions":{}}"#.into(),
            tokens: Tokens::estimate(r#"{"questions":{}}"#, None).unwrap(),
        },
        response: RawResponse {
            status,
            headers: Default::default(),
            body: body.into(),
        },
        reused: true,
        elapsed_ms: 0,
    }
}

#[test]
fn an_oversized_request_rejection_is_a_context_limit() {
    let rejected = r#"{"detail":"The state and question exceed the maximum context length."}"#;
    let error = receipt(400, rejected).judgment(0).unwrap_err();
    assert_eq!(error.code, "JEV_CONTEXT_LIMIT");
    assert_eq!(error.status, Some(400));
    let error = receipt(413, "request entity too large")
        .judgment(0)
        .unwrap_err();
    assert_eq!(error.code, "JEV_CONTEXT_LIMIT");
    let other = r#"{"error":"Unknown question type"}"#;
    assert_eq!(
        receipt(400, other).judgment(0).unwrap_err().code,
        "JEV_HTTP_STATUS"
    );
}
