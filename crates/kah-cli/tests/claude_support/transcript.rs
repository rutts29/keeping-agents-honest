//! Native hook transcript_path is authoritative; root and encoding are native.
//! The product resolver validates
//! the supplied pointer and uses the installed-source encoder as fallback.
use super::*;
pub(super) fn path(fixture: &Fixture) -> PathBuf {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    kah_cli::claude_transcript::resolve(&home, &fixture.root, &fixture.id, None)
        .expect("CLAUDE_FIXTURE_TRANSCRIPT stage=completion_poll")
}

#[test]
fn rows_use_the_native_hook_pointer_in_an_explicit_config_root() {
    if std::env::var_os("KAH_CLAUDE_TRANSCRIPT_UNIT_CHILD").is_some() {
        child();
        return;
    }
    let root = std::env::temp_dir().join(format!("claude-transcript-unit-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "claude_support::transcript::rows_use_the_native_hook_pointer_in_an_explicit_config_root", "--nocapture"])
        .env("KAH_CLAUDE_TRANSCRIPT_UNIT_CHILD", "1")
        .env("HOME", root.join("home"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("CLAUDE_CONFIG_DIR", root.join("native"))
        .status().unwrap();
    fs::remove_dir_all(root).unwrap();
    assert!(status.success());
}
fn child() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let root = home.join("repos/mine/kah-claude-live-with_underscore");
    fs::create_dir_all(&root).unwrap();
    let id = "00000000-0000-4000-8000-000000000001";
    let native = kah_claude::paths::config_root(&home)
        .join("projects/validated-hook")
        .join(format!("{id}.jsonl"));
    fs::create_dir_all(native.parent().unwrap().join(id).join("subagents")).unwrap();
    fs::write(&native, "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"CHILD_DONE\"}]}}\n").unwrap();
    fs::write(
        native
            .parent()
            .unwrap()
            .join(id)
            .join("subagents/child.jsonl"),
        "whole child record",
    )
    .unwrap();
    kah_cli::claude_transcript::register(
        &home,
        &root,
        &serde_json::json!({"session_id":id,"cwd":root,"transcript_path":native}),
    )
    .unwrap();
    let fixture = std::mem::ManuallyDrop::new(Fixture {
        created_root: root.clone(),
        root,
        id: id.into(),
        base: RunStore::default_state_root().unwrap(),
        evidence: home.join("evidence"),
        transcript_directory: home.join("wrong-old-path"),
    });
    assert_eq!(fixture.rows().len(), 1);
    assert_eq!(fixture.child_transcripts(), 1);
}
