use kah_claude::lifecycle::completed;
#[test]
fn final_text_and_stop_summary_are_not_yet_a_completed_turn() {
    let unfinished = "{\"type\":\"user\",\"sessionId\":\"parent\"}\n{\"type\":\"assistant\",\"sessionId\":\"parent\"}\n{\"type\":\"system\",\"subtype\":\"stop_hook_summary\"}\n";
    assert!(!completed(unfinished, "parent").unwrap());
    let finished = format!(
        "{unfinished}{{\"type\":\"system\",\"subtype\":\"turn_duration\",\"sessionId\":\"parent\"}}\n"
    );
    assert!(completed(&finished, "parent").unwrap());
    assert!(
        !completed(
            &format!("{finished}{{\"type\":\"user\",\"sessionId\":\"parent\"}}\n"),
            "parent"
        )
        .unwrap()
    );
    assert!(!completed(&format!("{unfinished}{{\"type\":\"system\",\"subtype\":\"turn_duration\",\"sessionId\":\"child\"}}\n"),"parent").unwrap());
    assert!(!completed(&format!("{finished}{{\"partial\":"), "parent").unwrap());
}
