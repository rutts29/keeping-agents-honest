//! Domain routing of a short Claude worker transcript: Mine and Research
//! refresh whole native sources with no action judgment; Untrusted routes to
//! commit checks. Runs in a child with a disposable home, so no model or Jev
//! call can succeed.
use kah_observer::{
    intake::{Decision, Input},
    session::{Binding, State, Store},
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};

const SESSION: &str = "5e55a1a0-0000-4000-8000-0000000000d4";

#[test]
fn mine_and_research_refresh_whole_native_sources_without_action_judgments() {
    let home = std::env::temp_dir().join(format!("kah-hook-routing-{}", std::process::id()));
    fs::create_dir_all(&home).unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "routing_child", "--nocapture"])
        .env("KAH_ROUTING_FIXTURE", &home)
        .env("HOME", &home)
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .output()
        .unwrap();
    fs::remove_dir_all(&home).unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn routing_child() {
    let Some(home) = std::env::var_os("KAH_ROUTING_FIXTURE") else {
        return;
    };
    let home = std::path::PathBuf::from(home);
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    assert!(base.starts_with(&home));
    let old = State::new(Binding {
        session: SESSION.into(),
        project: home.join("repos/mine/fixture"),
        codex_home: home.join(".codex"),
        native: "claude".into(),
        model: "claude-sonnet-5-5".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-latest".into(),
    });
    for domain in ["mine", "research"] {
        replay(&home, &base, &old, domain);
    }
    let mut untrusted = old.binding.clone();
    untrusted.project = home.join("repos/untrusted/fixture");
    assert!(kah_cli::observer_review::untrusted::host::scope(&untrusted).unwrap());
}

/// The user's goal, then one Bash tool use and its result.
fn transcript(project: &Path) -> String {
    let rows = [
        json!({"type":"user","uuid":"00000000-0000-4000-8000-0000000000e1",
            "message":{"role":"user","content":"Add a word-count command with tests. Never call the network."}}),
        json!({"type":"assistant","uuid":"00000000-0000-4000-8000-0000000000e2",
            "message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_wc_1","name":"Bash",
                "input":{"command":"cargo test word_count"}}]}}),
        json!({"type":"user","uuid":"00000000-0000-4000-8000-0000000000e3","toolUseResult":{"stdout":"test result: ok"},
            "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_wc_1","content":"test result: ok"}]}}),
    ];
    rows.into_iter()
        .map(|mut row| {
            row["sessionId"] = json!(SESSION);
            row["isSidechain"] = json!(false);
            row["cwd"] = json!(project);
            format!("{row}\n")
        })
        .collect()
}

fn replay(home: &Path, base: &Path, old: &State, domain: &str) {
    let project = home.join(format!("repos/{domain}/fixture"));
    fs::create_dir_all(&project).unwrap();
    let mut binding = old.binding.clone();
    binding.project = project.clone();
    let primary = Store::directory(base, &binding.session);
    kah_core::run_store::ensure_private_directory(&primary).unwrap();
    let mut state = old.clone();
    state.binding = binding.clone();
    let text = transcript(&project);
    let snapshot = kah_claude::transcript::parse(&text, &binding.session, &project).unwrap();
    assert!(!snapshot.actions.is_empty());
    let mut store = Store::open(base, &binding.session, Duration::ZERO).unwrap();
    store.save(&state).unwrap();
    // The goal was already taken in, so this sync needs no intake decision.
    let users = snapshot.users.as_ref().unwrap();
    store
        .add_source(&Input {
            source: users.source_refs()[0].clone(),
            local_record: None,
            text: users.records()[0].content().into(),
            decision: Some(Decision::Force),
            dismissed: false,
            promoted: false,
            rule_set_resolution: None,
            prior_intent: None,
            newer_conflicts: vec![],
        })
        .unwrap();
    drop(store);
    let outcome = kah_cli::observer_intake::snapshot(&binding, "stop", snapshot).unwrap();
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    assert!(outcome.projection.is_some());
    assert!(outcome.reminder.is_none() && outcome.verification.is_none());
    let after = Store::read(base, &binding.session).unwrap().unwrap();
    assert_eq!(after.usage.calls, old.usage.calls);
    assert_eq!(after.judgments.len(), old.judgments.len());
    check_tools(&binding, &primary);
    fs::remove_dir_all(primary).unwrap();
}

fn check_tools(binding: &Binding, primary: &Path) {
    let input: Value = json!({"hook_event_name":"PostToolUse","session_id":binding.session,
        "cwd":binding.project,"tool_use_id":"toolu_wc_1","tool_name":"Bash","tool_response":{}});
    assert_eq!(
        kah_cli::observer_review::untrusted::host::take(binding, &input).unwrap(),
        json!({})
    );
    kah_cli::observer_review::untrusted::host::schedule(binding, &input).unwrap();
    assert!(!primary.join("tool-jobs").exists());
    assert!(!primary.join("tool-events.jsonl").exists());
}
