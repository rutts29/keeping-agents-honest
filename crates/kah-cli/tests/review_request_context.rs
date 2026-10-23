use kah_cli::observer_review::request_context::{Context, read};

/// A Codex turn: its start marker, then the user message carrying a request.
fn codex_turn(turn: &str, request: &str) -> String {
    let started =
        serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":turn}});
    let prompt = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user",
        "content":[{"type":"input_text","text":format!("kah review request {request} (Drift). Read the evidence.")}]}});
    format!("{started}\n{prompt}\n")
}

#[test]
fn late_hook_remains_bound_to_its_request_after_the_next_request_is_published() {
    let rows = format!(
        "{}{}",
        codex_turn("turn-first", "review-1000-100"),
        codex_turn("turn-second", "review-2000-200")
    );
    let old = read(rows.as_bytes(), "codex", Some("turn-first"))
        .unwrap()
        .unwrap();
    assert_eq!(old.request, "review-1000-100");
    let next = read(rows.as_bytes(), "codex", Some("turn-second"))
        .unwrap()
        .unwrap();
    assert_eq!(next.request, "review-2000-200");
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

#[test]
fn deferred_claude_hook_reads_the_whole_native_prefix_seen_at_arrival() {
    use kah_cli::observer_review::{request_context::capture_at, state::State};
    let dir = std::env::temp_dir().join(format!("kah-hook-prefix-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("native.jsonl");
    let first = "{\"type\":\"user\",\"message\":{\"content\":\"kah review request review-old (Drift). Whole message.\"}}\n";
    std::fs::write(&path, first).unwrap();
    let end = first.len() as u64;
    std::fs::write(&path, format!("{first}{{\"type\":\"user\",\"message\":{{\"content\":\"kah review request review-next (Drift). Whole later message.\"}}}}\n")).unwrap();
    let state: State = serde_json::from_value(serde_json::json!({
        "worker":{"provider":"claude","session":"worker","project":dir,"pane":null,"socket":null},
        "observer":{"session":"observer","pane":null,"name":"observer","workspace":dir,"model":"claude-sonnet-5-5","effort":"low","transcript":path},
        "pending":null,"latest":null,"last_verified_turn":null,"checkpoint":null,"deferrals":[],"wakes":0,"native_tokens":0,"stopped":false
    })).unwrap();
    let input = serde_json::json!({"hook_event_name":"Stop"});
    assert_eq!(
        capture_at(&state, &input, Some(end))
            .unwrap()
            .unwrap()
            .request,
        "review-old"
    );
    assert_eq!(
        capture_at(&state, &input, None).unwrap().unwrap().request,
        "review-next"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
