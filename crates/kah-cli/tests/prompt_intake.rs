//! A Codex worker that does its whole task in one first turn gets intake at
//! its first prompt: no Stop is needed for the run, rules or commit intent.
//! Each test reruns itself with a private HOME whose `.local/bin/kah` only logs
//! detached job names, so no native or Jev process starts ($0).
use kah_cli::{
    observer_review::{option1, queue},
    prompt_intake,
};
use kah_core::{
    current_rules::Input as RuleInput,
    intent_history::{SourceProvider, SourceRole},
    observation::ObservationSnapshot,
    run_store::RunStore,
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use kah_observer::session::{self, Binding};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const PROMPT: &str = "steer: Add the README sentence, then commit it.";
const CHILD: &str = "KAH_PROMPT_INTAKE_CHILD";

fn in_private_home(name: &str) -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME").expect("run through demos/check-tests.sh");
    if let Some(home) = std::env::var_os(CHILD) {
        return Some(home.into());
    }
    let home =
        std::env::temp_dir().join(format!("prompt-intake-{}", kah_cli::relay_store::nonce()));
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(
        bin.join("kah"),
        "#!/bin/sh\necho \"$1\" >> \"$HOME/spawned.log\"\n",
    )
    .unwrap();
    fs::set_permissions(bin.join("kah"), fs::Permissions::from_mode(0o700)).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(CHILD, &home)
        .env("HOME", &home)
        .status()
        .unwrap();
    fs::remove_dir_all(&home).unwrap();
    assert!(status.success());
    None
}

fn fixture(home: &Path) -> Binding {
    let project = home.join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("AGENTS.md"), "Never truncate inputs.").unwrap();
    let binding = Binding {
        session: format!("prompt-intake-{}", kah_cli::relay_store::nonce()),
        project: project.canonicalize().unwrap(),
        codex_home: home.join(".codex"),
        native: "/unused".into(),
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
        timeout_secs: 90,
        judge: Default::default(),
        jev_model: "jev-1.13.0".into(),
    };
    let base = RunStore::default_state_root().unwrap();
    session::Store::open_wait(&base, &binding.session)
        .unwrap()
        .save(&session::State::new(binding.clone()))
        .unwrap();
    let review = kah_cli::observer_review::state::Store::directory(&binding.session).unwrap();
    kah_cli::unattended_report::save(&review, "config.json", &json!({"enabled":true,
        "provider":"codex","worker":binding.session,"turn":"first-turn","pid":null,"process_stamp":null}))
    .unwrap();
    binding
}

/// The native thread as Codex lists it once the first prompt is written.
fn recorded(binding: &Binding) -> ObservationSnapshot {
    let repository = format!(
        "sha256:{:x}",
        Sha256::digest(binding.project.as_os_str().as_encoded_bytes())
    );
    let users = NeutralSessionBundle::from_records(
        SourceProvider::Codex,
        binding.session.clone(),
        repository,
        vec![BundleRecord::new("item-1:0", SourceRole::User, PROMPT).unwrap()],
    )
    .unwrap();
    ObservationSnapshot {
        users: Some(users.clone()),
        native_candidates: Some(users),
        actions: vec![],
        action_digest: format!("sha256:{:x}", Sha256::digest(b"[]")),
    }
}

fn empty() -> ObservationSnapshot {
    ObservationSnapshot {
        users: None,
        native_candidates: None,
        actions: vec![],
        action_digest: format!("sha256:{:x}", Sha256::digest(b"[]")),
    }
}

fn prompt_hook(binding: &Binding, home: &Path) -> (Value, Duration) {
    let hook = json!({"hook_event_name":"UserPromptSubmit","turn_id":"turn-1","prompt":PROMPT});
    let start = Instant::now();
    prompt_intake::schedule(binding, &hook).unwrap();
    let latency = start.elapsed();
    let log = home.join("spawned.log");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !fs::read_to_string(&log).is_ok_and(|l| l.contains("__prompt-intake")) {
        assert!(
            Instant::now() < deadline,
            "prompt hook started no detached intake"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let request = prompt_intake::pending(&binding.session).unwrap().unwrap();
    (request, latency)
}

fn user_sources(binding: &Binding) -> usize {
    let base = RunStore::default_state_root().unwrap();
    let journal = session::Store::directory(&base, &binding.session).join("observations.jsonl");
    fs::read_to_string(journal)
        .unwrap()
        .lines()
        .filter(|l| l.contains("\"user_source_v1\"") && l.contains(PROMPT))
        .count()
}

#[test]
fn first_prompt_takes_intake_without_stop_and_its_commit_sees_that_intent() {
    let Some(home) =
        in_private_home("first_prompt_takes_intake_without_stop_and_its_commit_sees_that_intent")
    else {
        return;
    };
    let binding = fixture(&home);
    let (request, latency) = prompt_hook(&binding, &home);
    eprintln!("prompt hook added latency: {latency:?}");
    assert_eq!(request["turn"], "turn-1");
    assert_eq!(request["prompt_sha256"], prompt_intake::digest(PROMPT));
    // The new thread is unreadable, then readable without the prompt, then written.
    let mut reads = 0;
    let outcome = prompt_intake::observe_when_recorded(
        &binding,
        &request,
        Duration::from_secs(10),
        Duration::from_millis(1),
        || {
            reads += 1;
            match reads {
                1 => Err("CODEX_RPC_REJECTED stage=thread/read: thread not loaded".into()),
                2 => Ok(empty()),
                _ => Ok(recorded(&binding)),
            }
        },
    )
    .unwrap()
    .expect("intake must run once the native record exists");
    assert_eq!(reads, 3);
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    assert!(outcome.projection.is_some());
    let base = RunStore::default_state_root().unwrap();
    let state = session::Store::read(&base, &binding.session)
        .unwrap()
        .unwrap();
    let run = state
        .run_id
        .clone()
        .expect("first prompt must create the run");
    assert_eq!(user_sources(&binding), 1);
    let queued = queue::entries(&binding.session).unwrap();
    assert_eq!(queued.len(), 1, "one intent-sync rule rebuild is queued");
    assert_eq!(queued[0].1.trigger["kind"], "current_rules_rebuild");
    assert_eq!(queued[0].1.turn, "intent-sync");
    commit_uses_captured_intent(&binding);
    // Turn end for the same prompt: no second record, run or rebuild wake.
    let again = kah_cli::observer_intake::snapshot(&binding, "turn-1", recorded(&binding)).unwrap();
    assert!(again.gap.is_none(), "{:?}", again.gap);
    let state = session::Store::read(&base, &binding.session)
        .unwrap()
        .unwrap();
    assert_eq!(state.run_id.as_deref(), Some(run.as_str()));
    assert_eq!(user_sources(&binding), 1);
    assert_eq!(queue::entries(&binding.session).unwrap().len(), 1);
}

fn commit_uses_captured_intent(binding: &Binding) {
    let history = option1::authority::history(&binding.session)
        .expect("a commit in the same turn must find accepted intent");
    let standing = kah_observer::current_rules::standing_sources(&binding.project).unwrap();
    let rules = option1::direct::fallback(
        &RuleInput::from_history(&history, standing, "now").unwrap(),
        "observer",
    )
    .unwrap();
    let context = json!({"head":"b".repeat(40),"baseline_tree":"a".repeat(40),"touched_files":[
        {"path":"README.md","patch":"+sentence","deleted":false,"content":{"text":"sentence\n"}}]});
    let input = option1::input::build(&history, &rules, &context, true).unwrap();
    assert!(input.carried["intent_revision"].is_u64());
    assert!(!input.rules.contains_key("controller_intent_pending"));
    assert!(input.intent.goal.contains("Add the README sentence"));
}

#[test]
fn prompt_never_written_leaves_turn_end_intake_without_a_gap() {
    let Some(home) = in_private_home("prompt_never_written_leaves_turn_end_intake_without_a_gap")
    else {
        return;
    };
    let binding = fixture(&home);
    let (request, _) = prompt_hook(&binding, &home);
    let outcome = prompt_intake::observe_when_recorded(
        &binding,
        &request,
        Duration::from_millis(30),
        Duration::from_millis(1),
        || Ok(empty()),
    )
    .unwrap();
    assert!(outcome.is_none());
    let base = RunStore::default_state_root().unwrap();
    let state = session::Store::read(&base, &binding.session)
        .unwrap()
        .unwrap();
    assert!(state.run_id.is_none() && state.gap.is_none());
    assert!(queue::entries(&binding.session).unwrap().is_empty());
}
