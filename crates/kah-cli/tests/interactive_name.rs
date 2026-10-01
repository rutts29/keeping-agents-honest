use kah_cli::interactive_name;
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn claude_names_the_first_prompt_once_and_preserves_native_custom_names() {
    let root = std::env::temp_dir().join(format!("kah-name-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let marker = root.join("title.json");
    let input = json!({"hook_event_name":"UserPromptSubmit","prompt":"Fix TLS retries"});
    let name = interactive_name::claude_title(&marker, "uuid-one", Path::new("/project"), &input)
        .unwrap()
        .unwrap();
    assert!(name.contains("fix-tls-retries") && name.contains("uuid-one"));
    assert!(
        interactive_name::claude_title(&marker, "uuid-one", Path::new("/project"), &input)
            .unwrap()
            .is_none()
    );
    let explicit = root.join("explicit.json");
    interactive_name::claude_title(
        &explicit,
        "uuid-two",
        Path::new("/project"),
        &json!({"hook_event_name":"SessionStart","session_title":"User title"}),
    )
    .unwrap();
    assert!(
        interactive_name::claude_title(&explicit, "uuid-two", Path::new("/project"), &input)
            .unwrap()
            .is_none()
    );
    let delegate = root.join("delegate.json");
    assert!(
        interactive_name::claude_title(
            &delegate,
            "uuid-three",
            Path::new("/project"),
            &json!({"hook_event_name":"UserPromptSubmit","prompt":"[kah-builder] automation"})
        )
        .unwrap()
        .is_none()
    );
    assert!(!delegate.exists());
    fs::remove_dir_all(root).unwrap();
}
