use super::*;

#[test]
fn missing_root_is_reported_before_provider_execution() {
    let root = env::temp_dir().join(format!("kah-missing-root-{}", std::process::id()));
    assert!(!root.exists());
    let result = invoke_codex_request(&root, CompactorConfig::default(), json!({}), "fixture");
    let error = match result {
        Ok(_) => panic!("missing root accepted"),
        Err(error) => error,
    };
    assert!(format!("{error:?}").contains("OBSERVER_ROOT"));
}

#[test]
fn observer_tool_activity_is_rejected_instead_of_becoming_semantic_state() {
    let safe = vec![
        json!({"type":"thread.started"}),
        json!({"type":"turn.started"}),
        json!({"type":"item.completed","item":{"type":"agent_message","text":"{}"}}),
        json!({"type":"turn.completed"}),
    ];
    assert!(data_only_trajectory(&safe));
    for kind in [
        "command_execution",
        "file_change",
        "mcp_tool_call",
        "web_search",
        "todo_list",
    ] {
        let mut with_tool = safe.clone();
        with_tool.insert(2, json!({"type":"item.completed","item":{"type":kind}}));
        assert!(!data_only_trajectory(&with_tool));
    }
    let mut unknown = safe.clone();
    unknown.insert(2, json!({"type":"unknown_activity"}));
    assert!(!data_only_trajectory(&unknown));
    let mut warning = safe;
    warning.insert(
        1,
        json!({"type":"item.completed","item":{"type":"error","message":"startup warning"}}),
    );
    assert!(data_only_trajectory(&warning));
}
