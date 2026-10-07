use kah_cli::{native_binding, run_pane};
use kah_run::native::Native;
use serde_json::json;
use std::path::Path;

#[test]
fn native_resume_hook_is_authority_and_child_or_wrong_scope_is_not() {
    let id = "00000000-0000-0000-0000-000000000001";
    let mut input = json!({"hook_event_name":"SessionStart","source":"resume","session_id":id,"cwd":"/project",
        "transcript_path":format!("/home/test/.claude/projects/-project/{id}.jsonl"),"session_title":"kah-run-fixture"});
    let c = native_binding::claude_confirmation(
        &input,
        Path::new("/home/test"),
        Path::new("/project"),
        "launch",
    )
    .unwrap()
    .unwrap();
    assert_eq!(c.session, id);
    assert_eq!(c.source, "resume");
    assert_eq!(c.name.as_deref(), Some("kah-run-fixture"));
    input["agent_id"] = "child".into();
    assert!(
        native_binding::claude_confirmation(
            &input,
            Path::new("/home/test"),
            Path::new("/project"),
            "launch"
        )
        .unwrap()
        .is_none()
    );
    input.as_object_mut().unwrap().remove("agent_id");
    input["cwd"] = "/project/../other".into();
    assert!(
        native_binding::claude_confirmation(
            &input,
            Path::new("/home/test"),
            Path::new("/project"),
            "launch"
        )
        .is_err()
    );
}

#[test]
fn confirmed_claude_input_does_not_depend_on_herdrs_advisory_id() {
    let mut n = Native::new("claude").unwrap();
    n.session = Some("native".into());
    let pane = json!({"agent":"claude","agent_status":"idle","agent_session":{"value":"bootstrap","source":"pid-file"}});
    assert!(run_pane::accepts_confirmed_input(&pane, &n, true));
    assert!(!run_pane::accepts_confirmed_input(&pane, &n, false));
    n.provider = "codex".into();
    assert!(!run_pane::accepts_confirmed_input(&pane, &n, true));
}
#[test]
fn native_hook_path_with_underscores_and_non_alphanumerics_is_authoritative() {
    let id = "00000000-0000-4000-8000-000000000011";
    let project = Path::new("/mnt/kah-fixtures/project.name space+[é]😀");
    let path = format!(
        "/home/test/.claude/projects/-mnt-HC-Volume-106758648-project-name-space------/{id}.jsonl"
    );
    let input = json!({"hook_event_name":"SessionStart","source":"startup","session_id":id,
        "cwd":project,"transcript_path":path});
    let confirmed =
        native_binding::claude_confirmation(&input, Path::new("/home/test"), project, "launch")
            .unwrap()
            .unwrap();
    assert_eq!(
        confirmed.transcript,
        Path::new(input["transcript_path"].as_str().unwrap())
    );
}
#[test]
fn native_hook_path_rejects_escape_and_wrong_session_file() {
    let id = "00000000-0000-4000-8000-000000000012";
    for path in [
        format!("/home/test/.claude/projects/../secrets/{id}.jsonl"),
        "/home/test/.claude/projects/-project/wrong.jsonl".into(),
        format!("/outside/projects/-project/{id}.jsonl"),
    ] {
        let input = json!({"hook_event_name":"SessionStart","source":"startup","session_id":id,
            "cwd":"/project","transcript_path":path});
        assert!(
            native_binding::claude_confirmation(
                &input,
                Path::new("/home/test"),
                Path::new("/project"),
                "launch"
            )
            .is_err()
        );
    }
}
