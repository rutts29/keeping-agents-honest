//! Attach the same kah engine to an already-running native app conversation.
//! Uses native thread/inject_items, never turn/steer or a fabricated user record.
use crate::Result;
use kah_codex::{control::Control, source};
use kah_core::run_store::RunStore;
use kah_observer::{
    engine,
    judge::routing::Backend,
    session::{Binding, Store},
};
use serde_json::json;
use std::{fs, os::unix::fs::OpenOptionsExt, path::Path, time::Duration};

pub fn watch(home: &Path, project: &Path, session: &str) -> Result<()> {
    let project = project.canonicalize()?;
    if kah_sandbox::Project::select(&home.join("repos"), &project, None)?.domain
        != kah_sandbox::Domain::Mine
    {
        return Err("ATTACH_DOMAIN stage=attach: this bridge observes existing Mine sessions; use the kah launcher for restricted domains".into());
    }
    let native = home.join(".local/bin/codex").canonicalize()?;
    let base = RunStore::default_state_root()?;
    drop(Store::open(&base, session, Duration::ZERO)?);
    let directory = Store::directory(&base, session);
    let lease = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("watch.lock"))?;
    lease
        .try_lock()
        .map_err(|e| format!("OBSERVER_ALREADY_ATTACHED stage=attach: {e}"))?;
    let binding = Binding {
        session: session.into(),
        project: project.clone(),
        codex_home: home.join(".codex"),
        native: native.clone(),
        model: "gpt-6-luna".into(),
        effort: "low".into(),
        timeout_secs: 45,
        judge: Backend::Jev,
        jev_model: "jev-latest".into(),
    };
    let result = watch_loop(&binding);
    if let Err(error) = &result {
        let mut store = Store::open(&base, session, Duration::from_secs(1))?;
        let mut state = store
            .load()?
            .unwrap_or_else(|| kah_observer::session::State::new(binding));
        state.fail("OBSERVER_ATTACHMENT", error.to_string());
        store.save(&state)?;
        store.append(&json!({"kind":"gap","source":"app_attachment","gap":state.gap}))?;
    }
    result
}

fn watch_loop(binding: &Binding) -> Result<()> {
    let session = &binding.session;
    let mut client = Control::connect_daemon(
        &binding
            .codex_home
            .join("app-server-control/app-server-control.sock"),
        Duration::from_secs(20),
    )?;
    let mut last_projection = String::new();
    let mut gaps = 0;
    loop {
        let goal = client.request("thread/goal/get", json!({"threadId":session}))?;
        if goal["goal"]["status"]
            .as_str()
            .is_some_and(|status| status != "active")
        {
            eprintln!("kah observer detached: native goal is no longer active.");
            return Ok(());
        }
        let snapshot = source::observe(&mut client, &binding.project, session)?;
        let turn = snapshot.action_digest.clone();
        let outcome = engine::observe_snapshot(binding, &turn, true, snapshot)?;
        if let Some(gap) = outcome.gap {
            eprintln!("kah observer gap: {gap}");
            gaps += 1;
            if gaps >= 2 {
                return Err(gap.into());
            }
        } else {
            gaps = 0;
        }
        if let Some(projection) = outcome.projection {
            let text = projection.rendered_prompt();
            let reminder = outcome.reminder.map(|r| r.text);
            if text != last_projection || reminder.is_some() {
                let context = format!(
                    "[kah source-linked observer context, not new user steering. Current explicit user instructions take precedence.]\n{}\n{}",
                    text,
                    reminder.unwrap_or_default()
                );
                inject(&mut client, session, &context)?;
                last_projection = text;
                eprintln!(
                    "kah observer attached: session={session} revision={}",
                    projection.revision
                );
            }
        }
        std::thread::sleep(Duration::from_secs(30));
    }
}

pub fn inject(client: &mut Control, session: &str, context: &str) -> Result<()> {
    client.request(
        "thread/inject_items",
        json!({"threadId":session,"items":[{
            "type":"message","role":"developer","content":[{"type":"input_text","text":context}]
        }]}),
    )?;
    Ok(())
}
