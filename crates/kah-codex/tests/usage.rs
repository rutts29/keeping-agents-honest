use serde_json::json;
#[test]
fn cumulative_updates_restart_and_partial_tail_do_not_erase_consumption() {
    let mut text = [10, 10, 25, 4, 9].map(|n| json!({"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":n}}}}).to_string()).join("\n");
    text.push_str("\n{\"partial\":");
    assert_eq!(kah_codex::usage::tokens(text.as_bytes()).unwrap(), 34);
    assert!(kah_codex::usage::tokens(b"invalid\n".as_slice()).is_err());
}
