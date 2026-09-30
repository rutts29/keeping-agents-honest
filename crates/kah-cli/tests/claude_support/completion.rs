use serde_json::Value;

pub(super) fn completed_after(rows: &[Value], record: &str) -> bool {
    let id = record.split(':').next().unwrap_or(record);
    let Some(at) = rows.iter().rposition(|r| r["uuid"] == id) else {
        return false;
    };
    rows[at + 1..]
        .iter()
        .any(|r| r["type"] == "system" && r["subtype"] == "turn_duration")
}

#[test]
fn a_stop_hook_or_tool_result_does_not_prove_the_native_turn_finished() {
    use serde_json::json;
    let mut rows = vec![
        json!({"uuid":"user-1","type":"user"}),
        json!({"type":"system","subtype":"stop_hook_summary"}),
        json!({"type":"user","toolUseResult":{"content":"BETA"}}),
    ];
    assert!(!completed_after(&rows, "user-1:0"));
    rows.push(json!({"type":"system","subtype":"turn_duration"}));
    assert!(completed_after(&rows, "user-1:0"));
    rows.push(json!({"uuid":"user-2","type":"user"}));
    assert!(!completed_after(&rows, "user-2:0"));
}

#[test]
fn terminal_text_applies_cursor_overwrites() {
    assert!(super::screen("\x1b[2J\x1b[HSwitch mod l?\x1b[1;11He").contains("Switch model?"));
}
