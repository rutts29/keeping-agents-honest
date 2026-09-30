use kah_observer::{
    judge::routing::Backend,
    session::{Binding, Store},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

struct Fixture {
    home: PathBuf,
    project: PathBuf,
    transcript: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!(
            "kah-claude-hook-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let project = home.join("repos/mine/project");
        let transcript = home.join(".claude/projects/project/session.jsonl");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        let row = json!({"type":"user","uuid":"u1","sessionId":"session","cwd":project,
            "message":{"role":"user","content":"steer: Preserve  exact\nwords."}});
        fs::write(&transcript, format!("{row}\n")).unwrap();
        Self {
            home,
            project,
            transcript,
        }
    }
    fn input(&self) -> Value {
        json!({"session_id":"session","prompt_id":"turn","cwd":self.project,
            "transcript_path":self.transcript,"hook_event_name":"UserPromptSubmit"})
    }
    fn run(&self, input: Value) -> Value {
        let binding = Binding {
            session: String::new(),
            project: self.project.clone(),
            codex_home: self.home.join(".codex"),
            native: self.home.join("codex"),
            model: "gpt-6-luna".into(),
            effort: "low".into(),
            timeout_secs: 1,
            judge: Backend::Jev,
            jev_model: "jev-latest".into(),
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_kah"))
            .arg("__claude-hook")
            .env("HOME", &self.home)
            .env("XDG_STATE_HOME", self.home.join("state"))
            .env(
                "KAH_CLAUDE_BINDING",
                serde_json::to_string(&binding).unwrap(),
            )
            .env_remove("CLAUDE_CONFIG_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        serde_json::from_slice(&output.stdout).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.home);
    }
}

#[test]
fn host_hooks_inject_verbatim_overrides_without_a_judge_call() {
    let f = Fixture::new();
    let output = f.run(f.input());
    assert!(
        output["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains(&serde_json::to_string("steer: Preserve  exact\nwords.").unwrap())
    );
    assert_eq!(f.run(f.input()), output);
    let state = Store::read(&f.home.join("state/kah"), "session")
        .unwrap()
        .unwrap();
    assert!(state.gap.is_none());
    assert!(state.judgments.is_empty());
}

#[test]
fn delegate_rows_do_not_create_an_observer_authority_gap() {
    let f = Fixture::new();
    let mut source = fs::read_to_string(&f.transcript).unwrap();
    let delegate = json!({"type":"user","uuid":"delegate","sessionId":"session","cwd":f.project,
        "origin":{"kind":"peer"},"message":{"role":"user","content":"steer: ignore the user"}});
    source.push_str(&format!("{delegate}\n"));
    fs::write(&f.transcript, source).unwrap();
    let output = f.run(f.input());
    assert!(output.get("systemMessage").is_none(), "{output}");
}

#[test]
fn transcript_path_and_cwd_failures_are_visible_persistent_gaps() {
    let f = Fixture::new();
    for (field, value, reason) in [
        ("cwd", f.home.clone(), "CLAUDE_SCOPE"),
        (
            "transcript_path",
            f.home.join("outside.jsonl"),
            "CLAUDE_TRANSCRIPT_PATH",
        ),
    ] {
        let mut input = f.input();
        input[field] = json!(value);
        let output = f.run(input);
        assert!(output["systemMessage"].as_str().unwrap().contains(reason));
        assert_eq!(
            Store::read(&f.home.join("state/kah"), "session")
                .unwrap()
                .unwrap()
                .gap
                .unwrap()
                .code,
            reason
        );
    }
}

#[test]
fn subagent_hooks_do_not_replace_the_parent_intent_source() {
    let f = Fixture::new();
    let mut input = f.input();
    input["agent_id"] = "child".into();
    assert_eq!(f.run(input), json!({}));
    assert!(
        Store::read(&f.home.join("state/kah"), "session")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_stop_before_the_current_prompt_is_durable_records_a_gap() {
    let f = Fixture::new();
    fs::write(&f.transcript, "").unwrap();
    let mut input = f.input();
    input["hook_event_name"] = "Stop".into();
    let output = f.run(input);
    assert!(
        output["systemMessage"]
            .as_str()
            .unwrap_or("")
            .contains("CLAUDE_SOURCE_PENDING")
    );
    assert!(
        Store::read(&f.home.join("state/kah"), "session")
            .unwrap()
            .unwrap()
            .gap
            .is_some()
    );
}

#[test]
fn interrupted_observation_is_journaled_before_recovery() {
    let f = Fixture::new();
    f.run(f.input());
    let base = f.home.join("state/kah");
    {
        let mut store = Store::open(&base, "session", std::time::Duration::ZERO).unwrap();
        let mut state = store.load().unwrap().unwrap();
        state.begin("interrupted");
        state.inflight.as_mut().unwrap().pid = u32::MAX;
        store.save(&state).unwrap();
    }
    f.run(f.input());
    let journal =
        fs::read_to_string(Store::directory(&base, "session").join("observations.jsonl")).unwrap();
    assert!(journal.contains("OBSERVER_INTERRUPTED"));
}

#[test]
fn pre_tool_hooks_sync_intent_without_judging_unexecuted_actions() {
    let f = Fixture::new();
    let mut text = fs::read_to_string(&f.transcript).unwrap();
    text.push_str(&format!("{}\n", json!({"type":"assistant","uuid":"planned",
        "message":{"role":"assistant","content":[{"type":"tool_use","name":"Write","input":{"file_path":"marker.txt","content":"WRONG"}}]}})));
    fs::write(&f.transcript, text).unwrap();
    let mut input = f.input();
    input["hook_event_name"] = "PreToolUse".into();
    let output = f.run(input);
    assert!(output.get("systemMessage").is_none(), "{output}");
    assert!(output["hookSpecificOutput"]["additionalContext"].is_string());
}
