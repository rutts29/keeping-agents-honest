use kah_cli::observer_notice::{append, notice};
use serde_json::json;

#[test]
fn idle_notice_is_emitted_only_on_transitions_and_keeps_context() {
    let directory = std::env::temp_dir().join(format!("kah-idle-notice-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    assert_eq!(notice(&directory, true).unwrap(), Some(true));
    assert_eq!(notice(&directory, true).unwrap(), None);
    assert_eq!(notice(&directory, false).unwrap(), Some(false));
    assert_eq!(notice(&directory, false).unwrap(), None);
    assert_eq!(notice(&directory, true).unwrap(), Some(true));
    let mut output = json!({"systemMessage":"existing gap", "hookSpecificOutput":{
        "hookEventName":"SessionStart", "additionalContext":"original context"}});
    let context = output["hookSpecificOutput"].clone();
    append(&mut output, true);
    assert_eq!(output["hookSpecificOutput"], context);
    let message = output["systemMessage"].as_str().unwrap();
    assert!(message.starts_with("existing gap\n"));
    assert!(message.contains("observer idle"));
    assert!(message.contains("empty"));
    std::fs::remove_dir_all(directory).unwrap();
}
