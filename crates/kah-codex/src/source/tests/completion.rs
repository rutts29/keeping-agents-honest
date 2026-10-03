use super::*;

#[test]
fn followup_without_tools_keeps_prior_activity_and_counts_omitted_kinds() {
    let entries = vec![
        json!({"turnId":"first","item":{"id":"patch","type":"fileChange","status":"completed","changes":[]}}),
        json!({"turnId":"first","item":{"id":"mcp","type":"mcpToolCall"}}),
        json!({"turnId":"second","item":{"id":"reply","type":"agentMessage","text":"Already done"}}),
    ];
    let actions = completion_actions(&entries).unwrap();
    assert_eq!(actions[0]["evidence"]["byKind"]["mcpToolCall"], 1);
    assert_eq!(actions[0]["evidence"]["complete"], true);
    assert_eq!(actions[1]["id"], "patch");
    assert_eq!(actions[1]["turn_id"], "first");
    assert_eq!(actions[2]["id"], "reply");
    assert_eq!(actions[2]["kind"], "agentMessage");
    assert_eq!(actions.len(), 3);
}

#[test]
fn all_completed_tools_keep_whole_output_including_the_last_sentence() {
    let output = format!(
        "{}\nFINAL FAILURE: required integration is missing.",
        "full output 完整 ".repeat(1000)
    );
    let entries=(0..30).map(|n|json!({"turnId":"t","item":{"id":format!("tool-{n}"),"type":"commandExecution","status":"completed","exitCode":1,"aggregatedOutput":output}})).collect::<Vec<_>>();
    let actions = completion_actions(&entries).unwrap();
    assert_eq!(actions.len(), 31);
    assert_eq!(actions[1]["evidence"]["output"], output);
    assert_eq!(actions[30]["evidence"]["output"], output);
    assert_eq!(actions[30]["truncated"], false);
}
