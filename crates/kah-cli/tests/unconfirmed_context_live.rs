//! Seed uncertainty, then exercise the real hook and native context channel.
#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
#[path = "journeys/fixture.rs"]
mod fixture;
use client::Client;
use kah_core::{
    intent_history::Event,
    projection::{ActiveProjection, CompactedIntent},
    run_store::RunStore,
};
use kah_observer::{compaction, engine, session::Store};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    time::Duration,
};

#[test]
#[ignore = "native agent receives seeded unconfirmed text through production hook after compaction"]
fn unconfirmed_context_is_visible_without_user_review() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut fixture) = fixture::prepare(&home, "research", false);
    let mut client = Client::start_command(launch.command(true), &fixture.stderr);
    let start = client.request("thread/start", json!({"cwd":fixture.project}));
    let session = start["thread"]["id"].as_str().unwrap();
    fixture.sessions.push(session.into());
    assert_eq!(client.turn(session,"steer: Retain marker UNCONFIRMED_BETA for this disposable fixture. Reply READY without tools.")["status"],"completed");
    let state = seed_unconfirmed(session);
    let mut hook = Command::new(env!("CARGO_BIN_EXE_kah"))
        .arg("__hook")
        .envs(
            launch
                .command(true)
                .get_envs()
                .filter_map(|(k, v)| v.map(|v| (k, v))),
        )
        .env(
            "KAH_OBSERVER_BINDING",
            serde_json::to_string(&state.binding).unwrap(),
        )
        .env_remove("CODEX_THREAD_ID")
        .env_remove("HERDR_ENV")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    hook.stdin.take().unwrap().write_all(json!({"session_id":session,"cwd":fixture.project,"hook_event_name":"UserPromptSubmit","turn_id":"seeded-context"}).to_string().as_bytes()).unwrap();
    let result = hook.wait_with_output().unwrap();
    assert!(result.status.success());
    let output: Value = serde_json::from_slice(&result.stdout).unwrap();
    let context = output["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("unconfirmed_items") && context.contains("UNCONFIRMED_BETA"));
    assert!(context.contains("\"review_required\":true"));
    client.request("thread/inject_items",json!({"threadId":session,"items":[{"type":"message","role":"developer","content":[{"type":"input_text","text":context}]}]}));
    assert_eq!(client.turn(session,"aside: Inspect the kah unconfirmed context. Use native apply_patch to write context.json with keys marker (the retained marker), review_required (the item's flag), and user_confirmed (false if absent). Do not ask for review or change the ledger. Report and stop.")["status"],"completed");
    let observed: Value =
        serde_json::from_slice(&fs::read(fixture.project.join("context.json")).unwrap()).unwrap();
    assert_eq!(
        observed,
        json!({"marker":"UNCONFIRMED_BETA","review_required":true,"user_confirmed":false})
    );
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    let directory = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(directory.join("unconfirmed-context.json"),serde_json::to_string_pretty(&json!({"passed":true,"uncertaintySeeded":true,"compactionSummarySeeded":true,"productionHookRenderedExactUnconfirmedItem":true,"nativeContextInjection":true,"nativeAgentObserved":observed,"userReviewActions":0,"globalConfigUnchanged":true,"classificationNotTestedByThisJourney":true})).unwrap()).unwrap();
}
fn seed_unconfirmed(session: &str) -> kah_observer::session::State {
    let base = RunStore::default_state_root().unwrap();
    let mut journal = Store::open(&base, session, Duration::from_secs(5)).unwrap();
    let mut state = journal.load().unwrap().unwrap();
    let store =
        RunStore::open_default(&state.binding.project, state.run_id.as_ref().unwrap()).unwrap();
    let mut recovered = store.recover().unwrap();
    let mut item = recovered.history.intents().values().next().unwrap().clone();
    item.id = "seeded-unconfirmed".into();
    item.review_required = true;
    recovered
        .history
        .append(Event::RequirementAdded {
            item,
            approval: None,
        })
        .unwrap();
    store
        .append_pending(recovered.history.records().last().unwrap().clone())
        .unwrap();
    let projection = ActiveProjection::from_history(&recovered.history, 32768).unwrap();
    store.commit_projection(&projection).unwrap();
    state.compaction = Some(CompactedIntent {
        source_digest: compaction::digest(&projection),
        summary: "Fixture context; retrieve originals for exact marker.".into(),
        originals: projection
            .frontier
            .iter()
            .map(|i| i.id.to_string())
            .collect(),
        model: "seeded-test-summary".into(),
    });
    journal.save(&state).unwrap();
    drop(store);
    drop(journal);
    assert!(
        engine::projection(&state)
            .unwrap()
            .unwrap()
            .rendered_prompt()
            .contains("unconfirmed_items")
    );
    state
}
