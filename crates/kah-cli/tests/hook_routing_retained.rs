//! Domain routing replay of the retained early Claude worker and its intake decisions.
use kah_observer::session::{Binding, State, Store};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

#[test]
#[ignore = "retained native transcript; offline, no model inference"]
fn mine_and_research_refresh_whole_native_sources_without_action_judgments() {
    let corpus = std::path::PathBuf::from(std::env::var_os("KAH_ROUTING_CORPUS").unwrap());
    let home = std::path::PathBuf::from(std::env::var_os("HOME").unwrap());
    assert!(home.starts_with("/mnt/kah-fixtures/cargo/test-fixtures"));
    let base = kah_core::run_store::RunStore::default_state_root().unwrap();
    assert!(base.starts_with(&home));
    let directory = corpus.join("home/.local/state/kah/observer-sessions/000000000000000000000004");
    let old: State =
        serde_json::from_slice(&fs::read(directory.join("state.json")).unwrap()).unwrap();
    let native: Value =
        serde_json::from_slice(&fs::read(directory.join("native-transcript.json")).unwrap())
            .unwrap();
    let source = std::path::PathBuf::from(native["transcript"].as_str().unwrap());
    assert!(
        source.starts_with(&corpus),
        "ROUTING_SOURCE_SCOPE stage=replay"
    );
    let transcript = fs::read(&source).unwrap();
    let journal = directory.join("observations.jsonl");
    let mut results = vec![];
    for domain in ["mine", "research"] {
        results.push(replay(&home, &base, &old, &transcript, &journal, domain));
    }
    let mut untrusted = old.binding.clone();
    untrusted.project = home.join("repos/untrusted/fixture");
    assert!(kah_cli::observer_intake::legacy_checks(&untrusted).unwrap());
    let evidence = std::path::PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::create_dir_all(&evidence).unwrap();
    kah_cli::unattended_report::save(&evidence, "routing.json", &json!({"passed":true,"results":results,
        "source":source,"source_sha256":format!("{:x}",Sha256::digest(&transcript)),
        "untrusted_legacy_route":true,"native_calls":0,"real_jev_calls":0,"final_acceptance":false})).unwrap();
}

fn replay(
    home: &Path,
    base: &Path,
    old: &State,
    transcript: &[u8],
    journal: &Path,
    domain: &str,
) -> Value {
    let project = home.join(format!("repos/{domain}/fixture"));
    fs::create_dir_all(&project).unwrap();
    let mut binding = old.binding.clone();
    binding.project = project.clone();
    binding.codex_home = home.join(".codex");
    let primary = Store::directory(base, &binding.session);
    kah_core::run_store::ensure_private_directory(&primary).unwrap();
    let mut state = old.clone();
    state.binding = binding.clone();
    state.run_id = None;
    let mut store = Store::open(base, &binding.session, Duration::ZERO).unwrap();
    store.save(&state).unwrap();
    fs::copy(journal, primary.join("observations.jsonl")).unwrap();
    drop(store);
    let text = relocate(transcript, &project);
    let snapshot = kah_claude::transcript::parse(&text, &binding.session, &project).unwrap();
    assert!(!snapshot.actions.is_empty());
    let before = serde_json::to_value(&snapshot.actions).unwrap();
    let outcome =
        kah_cli::observer_intake::snapshot(&binding, "retained:stop", true, snapshot).unwrap();
    assert!(outcome.gap.is_none(), "{:?}", outcome.gap);
    assert!(outcome.projection.is_some());
    assert!(outcome.reminder.is_none() && outcome.verification.is_none());
    let after = Store::read(base, &binding.session).unwrap().unwrap();
    assert_eq!(after.usage.calls, old.usage.calls);
    assert_eq!(after.judgments.len(), old.judgments.len());
    check_tools(&binding, &primary);
    let result = json!({"domain":domain,"actions":before.as_array().unwrap().len(),
            "source_words_preserved":true,"historical_usage":old.usage,"new_jev_calls":0,"new_judgments":0});
    fs::remove_dir_all(primary).unwrap();
    result
}

fn relocate(transcript: &[u8], project: &Path) -> String {
    String::from_utf8(transcript.to_vec())
        .unwrap()
        .lines()
        .map(|line| {
            let mut row: Value = serde_json::from_str(line).unwrap();
            if row.get("cwd").is_some() {
                row["cwd"] = json!(project);
            }
            serde_json::to_string(&row).unwrap()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn check_tools(binding: &Binding, primary: &Path) {
    let input = json!({"hook_event_name":"PostToolUse","session_id":binding.session,
        "cwd":binding.project,"tool_use_id":"retained-tool","tool_name":"Bash","tool_response":{}});
    assert_eq!(
        kah_cli::in_turn_worker::schedule(binding, &input, true).unwrap(),
        json!({})
    );
    assert!(
        kah_cli::in_turn_worker::pending(binding, &input)
            .unwrap()
            .is_none()
    );
    assert!(!primary.join("tool-jobs").exists());
    assert!(!primary.join("tool-events.jsonl").exists());
}
