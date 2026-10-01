use serde_json::json;
#[test]
fn native_message_revisions_count_cache_and_output_once() {
    let mut text = [("a", 2), ("a", 7), ("b", 3)].map(|(id, output)| json!({"type":"assistant","message":{"id":id,"usage":{"input_tokens":10,"cache_creation_input_tokens":20,"cache_read_input_tokens":30,"output_tokens":output}}}).to_string()).join("\n");
    text.push_str("\n{\"partial\":");
    assert_eq!(kah_claude::usage::tokens(text.as_bytes()).unwrap(), 130);
    assert!(kah_claude::usage::tokens(b"invalid\n".as_slice()).is_err());
}
