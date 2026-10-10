//! Restart only genuinely empty native sessions; never discard accepted history.
use crate::{Result, native_history::History};
use kah_run::store::Store;
use std::{
    collections::VecDeque,
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

pub fn prepare(home: &Path, base: &Path, id: &str) -> Result<()> {
    let record = Store::read(base, id)?;
    let native = record
        .native
        .as_ref()
        .ok_or("RUN_NATIVE_STATE stage=resume")?;
    let Some(session) = &native.session else {
        return Ok(());
    };
    let directory = Store::path(base, id)?;
    let durable = has_user(home, &directory, &record, session)?;
    if durable {
        return reconcile(home, &directory, base, id, &record, session);
    }
    if native.source_cursor > 0 || native.native_tokens > 0 || native.observer_tokens > 0 {
        return Err("RUN_NATIVE_HISTORY_MISSING stage=resume: previously accepted history is missing; refusing to replace its session".into());
    }
    let mut store = Store::open_wait(base, id)?;
    let mut current = store.load()?;
    let n = current.native.as_mut().unwrap();
    if n.session.as_ref() != Some(session) {
        return Err("RUN_RESUME_CHANGED stage=resume".into());
    }
    let mut pending = n
        .pending
        .iter()
        .chain(n.submitted.iter())
        .copied()
        .chain(n.sending)
        .filter(|s| *s != 0)
        .collect::<Vec<_>>();
    pending.sort_unstable();
    pending.dedup();
    n.pending = pending.into();
    n.submitted = VecDeque::from([0]);
    n.sending = None;
    n.session = None;
    n.confirmation = None;
    n.processed_digest = None;
    crate::unattended_report::save(
        &directory,
        &format!("empty-session-{}.json", kah_observer::session::now_ms()),
        &serde_json::json!({"previousNativeSession":session,"reason":"no durable primary user prompt","taskAndCountersPreserved":true}),
    )?;
    store.save(&current)?;
    Ok(())
}
fn reconcile(
    home: &Path,
    directory: &Path,
    base: &Path,
    id: &str,
    record: &kah_run::store::Record,
    session: &str,
) -> Result<()> {
    let started = std::time::Instant::now();
    let native = record.native.as_ref().unwrap();
    let mut history = History::open(home, directory, &native.provider, &record.worktree, session)?;
    let state_root = kah_core::run_store::RunStore::default_state_root()?;
    let state = kah_observer::session::Store::read(&state_root, session)?
        .ok_or("RUN_OBSERVER_MISSING stage=resume")?;
    if state.binding.project != record.worktree {
        return Err("RUN_OBSERVER_SCOPE stage=resume".into());
    }
    let outcome =
        crate::observer_intake::snapshot(&state.binding, "run:resume", false, history.snapshot()?)?;
    if let Some(gap) = outcome.gap {
        return Err(format!("RUN_OBSERVER_GAP stage=resume: {gap}").into());
    }
    let observer = kah_observer::session::Store::open(
        &state_root,
        session,
        std::time::Duration::from_secs(1),
    )?;
    let texts = observer
        .sources()?
        .into_iter()
        .map(|s| s.text)
        .collect::<Vec<_>>();
    let usage = observer.load()?.unwrap().usage;
    drop(observer);
    let tokens = history.tokens()?;
    let mut store = Store::open_wait(base, id)?;
    let mut record = store.load()?;
    let n = record.native.as_mut().unwrap();
    n.sources(&mut record.control, &texts)?;
    let mut pending = n
        .pending
        .iter()
        .chain(n.submitted.iter())
        .copied()
        .chain(n.sending)
        .collect::<Vec<_>>();
    pending.sort_unstable();
    pending.dedup();
    n.pending = pending.into();
    n.submitted.clear();
    n.sending = None;
    n.account(
        &mut record.control,
        tokens,
        usage.input_tokens.saturating_add(usage.output_tokens),
        started.elapsed().as_millis().min(u64::MAX as u128) as u64,
    );
    store.save(&record)?;
    Ok(())
}
fn has_user(
    home: &Path,
    directory: &Path,
    record: &kah_run::store::Record,
    session: &str,
) -> Result<bool> {
    let native = record.native.as_ref().unwrap();
    Ok(if native.provider == "claude" {
        let path = crate::claude_transcript::resolve(
            home,
            &record.worktree,
            session,
            native.confirmation.as_ref().map(|c| c.transcript.as_path()),
        )?;
        match fs::File::open(path) {
            Ok(file) => claude_reader(BufReader::new(file), session, &record.worktree)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
            Err(e) => return Err(e.into()),
        }
    } else {
        match History::open(home, directory, &native.provider, &record.worktree, session) {
            Ok(mut history) => history
                .snapshot()?
                .users
                .is_some_and(|u| !u.authoritative_records().is_empty()),
            Err(e) if missing_thread(e.as_ref(), session) => false,
            Err(e) => return Err(e),
        }
    })
}

pub fn claude_has_user(text: &str, session: &str, project: &Path) -> Result<bool> {
    claude_reader(text.as_bytes(), session, project)
}
fn claude_reader(mut reader: impl BufRead, session: &str, project: &Path) -> Result<bool> {
    use std::io::Read;
    loop {
        let mut line = vec![];
        if reader
            .by_ref()
            .take(32 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)?
            == 0
        {
            return Ok(false);
        }
        if line.len() > 32 * 1024 * 1024 {
            return Err("RUN_HISTORY_BOUND stage=resume".into());
        }
        let parsed = std::str::from_utf8(&line)
            .map_err(|e| e.to_string())
            .and_then(|s| kah_claude::transcript::parse(s, session, project));
        match parsed {
            Ok(snapshot)
                if snapshot
                    .users
                    .as_ref()
                    .is_some_and(|u| !u.authoritative_records().is_empty()) =>
            {
                return Ok(true);
            }
            Ok(_) => (),
            Err(_) if !line.ends_with(b"\n") => return Ok(false),
            Err(e) => return Err(e.into()),
        }
    }
}
pub fn missing_thread(error: &(dyn std::error::Error + 'static), id: &str) -> bool {
    let Some(e) = error.downcast_ref::<kah_codex::control::ControlError>() else {
        return false;
    };
    if e.code != "CODEX_RPC_REJECTED" {
        return false;
    }
    let Some(value) = e
        .detail
        .strip_prefix("thread/read: ")
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
    else {
        return false;
    };
    value["code"] == -32600
        && [
            format!("thread not loaded: {id}"),
            format!("thread not found: {id}"),
            format!("no rollout found for thread id {id}"),
        ]
        .iter()
        .any(|s| value["message"].as_str() == Some(s))
}
