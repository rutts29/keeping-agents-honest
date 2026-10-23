use kah_cli::native_history::History;
use serde_json::json;
use std::{fs, path::Path};
#[test]
fn native_history_reads_native_encoded_project_directory() {
    let home = std::env::temp_dir().join(format!("kah-path-unit-{}", std::process::id()));
    let project = Path::new("/fixture/project.name space+[é]😀");
    let session = "00000000-0000-4000-8000-000000000011";
    let directory = home.join(".claude/projects/-fixture-project-name-space------");
    fs::create_dir_all(&directory).unwrap();
    let row = json!({"type":"user","uuid":"user-one","sessionId":session,"cwd":project,
        "message":{"role":"user","content":"steer: Preserve BLUE."}});
    fs::write(
        directory.join(format!("{session}.jsonl")),
        format!("{row}\n"),
    )
    .unwrap();
    let mut history =
        History::open(&home, &home.join("inspection"), "claude", project, session).unwrap();
    let snapshot = history.snapshot().unwrap();
    assert_eq!(snapshot.users.unwrap().authoritative_records().len(), 1);
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn history_prefers_validated_hook_path_even_when_storage_layout_differs() {
    let home = std::env::temp_dir().join(format!("kah-path-hook-unit-{}", std::process::id()));
    let project = Path::new("/mnt/project_with_underscores");
    let session = "00000000-0000-4000-8000-000000000013";
    let directory = home.join(".claude/projects/native-owned-original-project");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{session}.jsonl"));
    let row = json!({"type":"user","uuid":"real-user","sessionId":session,"cwd":project,
        "message":{"role":"user","content":"steer: Keep BLUE."}});
    fs::write(&path, format!("{row}\n")).unwrap();
    kah_cli::claude_transcript::register(
        &home,
        project,
        &json!({"hook_event_name":"SessionStart", "cwd":project,
        "session_id":session,"transcript_path":path}),
    )
    .unwrap();
    let mut history =
        History::open(&home, &home.join("inspection"), "claude", project, session).unwrap();
    assert_eq!(
        history
            .snapshot()
            .unwrap()
            .users
            .unwrap()
            .authoritative_records()
            .len(),
        1
    );
    fs::remove_dir_all(home).unwrap();
}
#[test]
fn host_pointer_rejects_symlink_outside_native_projects() {
    let home = std::env::temp_dir().join(format!("kah-path-link-unit-{}", std::process::id()));
    let directory = home.join(".claude/projects/native");
    fs::create_dir_all(&directory).unwrap();
    fs::write(home.join("outside.jsonl"), "unrelated data").unwrap();
    let session = "00000000-0000-4000-8000-000000000014";
    let path = directory.join(format!("{session}.jsonl"));
    std::os::unix::fs::symlink(home.join("outside.jsonl"), &path).unwrap();
    assert!(kah_cli::claude_transcript::validate(&home, session, &path).is_err());
    fs::remove_dir_all(home).unwrap();
}
