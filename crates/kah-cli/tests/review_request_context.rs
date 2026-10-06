use kah_cli::observer_review::request_context::{Context, read};

#[test]
fn real_old_stop_remains_bound_to_its_request_after_the_next_request_is_published() {
    let rows = include_str!("fixtures/late-observer-stop.jsonl");
    let old = read(
        rows.as_bytes(),
        "codex",
        Some("00000000-0000-4000-8000-100000000002"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(old.request, "review-1000000000001-1");
    let next = read(
        rows.as_bytes(),
        "codex",
        Some("00000000-0000-4000-8000-100000000003"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(next.request, "review-1000000000002-1");
    assert_ne!(old, next);
}

#[test]
fn claude_native_paste_receipts_skip_meta_and_sidechain_messages() {
    let rows = concat!(
        "{\"type\":\"user\",\"message\":{\"content\":\"<pasted_content id=\\\"1\\\">\\nkah review request review-old (Drift). Full request.\\n</pasted_content id=\\\"1\\\">\"}}\n",
        "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":\"kah review request review-meta (Drift).\"}}\n",
        "{\"type\":\"user\",\"isSidechain\":true,\"message\":{\"content\":\"kah review request review-child (Drift).\"}}\n"
    );
    assert_eq!(
        read(rows.as_bytes(), "claude", None).unwrap(),
        Some(Context {
            request: "review-old".into(),
            preparation: false
        })
    );
}

#[test]
fn current_preparation_turn_counts_as_progress_but_an_older_review_does_not() {
    let context = kah_cli::observer_review::request_context::Context {
        request: "review-current-v8-decision".into(),
        preparation: true,
    };
    assert!(context.bound_to("review-current", "review-current-v8-decision"));
    assert!(!context.bound_to("review-next", "review-next-v8-decision"));
}
