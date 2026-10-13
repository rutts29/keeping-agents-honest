use super::{Proof, verify};
use crate::{Result, observer_review as review};
use review::state::{State, Store};
use serde_json::{Value, json};
use std::{
    fs,
    io::BufReader,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

fn unbound(detail: impl std::fmt::Display) -> Box<dyn std::error::Error> {
    format!("REVIEW_NATIVE_UNBOUND stage=observer_binding: {detail}").into()
}
pub fn ensure(worker: &str) -> Result<()> {
    let Some(state) = Store::read(worker)? else {
        return Ok(());
    };
    if state.pending.is_none() || state.observer.pane.is_none() || state.worker.provider != "codex"
    {
        return Ok(());
    }
    if state.observer.session.is_some() && state.observer.transcript.is_some() {
        if Store::directory(worker)?
            .join("native-binding-receipts")
            .join(format!("{}.json", state.pending.as_ref().unwrap().id))
            .is_file()
        {
            recheck_bound_pane(&state)?;
        }
        return continue_ready(worker, acknowledge_bound(&state)?);
    }
    review::control::owned_pane(&state).map_err(unbound)?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or_else(|| unbound("owned pane missing"))?;
    let socket = state
        .worker
        .socket
        .as_deref()
        .ok_or_else(|| unbound("owned socket missing"))?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    let session = pane_session(&state, &view["result"]["pane"])?;
    let (path, proof) = read_proof(&state, &session)?;
    // Recheck ownership after reading: neither a replaced pane nor newer request
    // can inherit an earlier native receipt.
    review::control::owned_pane(&state).map_err(unbound)?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    if pane_session(&state, &view["result"]["pane"])? != session {
        return Err(unbound("native identity changed during receipt read"));
    }
    let ready = persist(&state, &path, &proof)?;
    continue_ready(worker, ready)
}
fn continue_ready(worker: &str, ready: bool) -> Result<()> {
    review::provider_block::observe(worker)?;
    review::runtime::require_active(worker)?;
    if ready {
        review::hook::spawn("__review-consume", worker)?;
    }
    Ok(())
}
fn recheck_bound_pane(state: &State) -> Result<()> {
    review::control::owned_pane(state).map_err(unbound)?;
    let socket = state
        .worker
        .socket
        .as_deref()
        .ok_or_else(|| unbound("owned socket missing"))?;
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or_else(|| unbound("owned pane missing"))?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    pane_session(state, &view["result"]["pane"])?;
    Ok(())
}
pub(super) fn pane_session(state: &State, pane: &Value) -> Result<String> {
    if pane["pane_id"].as_str() != state.observer.pane.as_deref()
        || pane["agent"] != "codex"
        || pane["agent_session"]["source"] != "herdr:codex"
        || pane["agent_session"]["kind"] != "id"
        || pane["foreground_cwd"]
            .as_str()
            .is_none_or(|s| !review::control::contains_workspace(&state.observer.workspace, s))
    {
        return Err(unbound("pane identity is not the owned native observer"));
    }
    let id = pane["agent_session"]["value"]
        .as_str()
        .filter(|id| id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'))
        .ok_or_else(|| unbound("native session id missing"))?;
    if state
        .observer
        .session
        .as_deref()
        .is_some_and(|expected| expected != id)
    {
        return Err(unbound("owned observer session differs"));
    }
    Ok(id.into())
}
fn locate(root: &Path, session: &str) -> Result<Option<PathBuf>> {
    if !root.exists() {
        return Ok(None);
    }
    let root = root.canonicalize()?;
    let mut directories = vec![root.clone()];
    let mut found = None;
    let suffix = format!("-{session}.jsonl");
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                directories.push(entry.path());
            } else if kind.is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(&suffix))
            {
                let path = entry.path().canonicalize()?;
                if !path.starts_with(&root) || found.is_some() {
                    return Err(unbound("native source is ambiguous or outside its root"));
                }
                found = Some(path);
            }
        }
    }
    Ok(found)
}
pub(super) fn persist(captured: &State, path: &Path, proof: &Proof) -> Result<bool> {
    let store = Store::open_wait(&captured.worker.session)?;
    let mut state = store
        .load()?
        .ok_or_else(|| unbound("worker state disappeared"))?;
    if state.stopped
        || serde_json::to_value(&state.worker)? != serde_json::to_value(&captured.worker)?
        || serde_json::to_value(&state.pending)? != serde_json::to_value(&captured.pending)?
        || state.observer.pane != captured.observer.pane
        || state.observer.workspace != captured.observer.workspace
        || state
            .observer
            .session
            .as_deref()
            .is_some_and(|id| id != proof.session)
    {
        return Err(unbound("request or observer binding changed before save"));
    }
    let request = &captured
        .pending
        .as_ref()
        .ok_or_else(|| unbound("pending request missing"))?
        .id;
    let root = store.directory.join("native-binding-receipts");
    crate::unattended_report::save(
        &root,
        &format!("{request}.json"),
        &json!({"request":request,"pane":state.observer.pane,"session":proof.session,"provider":"codex",
        "transcript":path,"proof":proof,"provenance":"owned_pane_session_meta_and_native_request_receipt","at_ms":kah_observer::session::now_ms()}),
    )?;
    state.observer.session = Some(proof.session.clone());
    state.observer.transcript = Some(path.into());
    store.save(&state)?;
    let submitted = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{request}.json"));
    if !submitted.is_file() {
        return Ok(false);
    }
    // Submission validation remains in the ordinary consume path.
    review::consume::read_report(&submitted)?;
    crate::unattended_report::save(
        &store.directory,
        "report-ready.json",
        &json!({"request":request}),
    )?;
    review::timing::now(&state.worker.session, request, "acknowledged")?;
    Ok(true)
}

fn read_proof(state: &State, session: &str) -> Result<(PathBuf, Proof)> {
    let worker = &state.worker.session;
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?
    .ok_or_else(|| unbound("worker native home missing"))?
    .binding;
    let path = locate(&binding.codex_home.join("sessions"), session)?
        .ok_or_else(|| unbound(format!("native rollout missing for {session}")))?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|e| unbound(format!("{}: {e}", path.display())))?;
    if !file.metadata()?.is_file() {
        return Err(unbound("native source is not a regular file"));
    }
    let decision =
        review::preparation::active_decision(worker, &state.pending.as_ref().unwrap().id)?;
    let marker = format!("kah review request {decision} ");
    let proof = verify(
        BufReader::new(file),
        session,
        &state.observer.workspace,
        &marker,
    )?
    .ok_or_else(|| unbound("exact request has no native receipt yet"))?;
    Ok((path, proof))
}

pub(super) fn acknowledge_bound(state: &State) -> Result<bool> {
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    let root = Store::directory(&state.worker.session)?;
    let submitted = state
        .observer
        .workspace
        .join("outbox")
        .join(format!("submitted-{}.json", pending.id));
    if !submitted.is_file() {
        return Ok(false);
    }
    let ready: Option<Value> = review::state::read(&root.join("report-ready.json"))?;
    if ready.is_some_and(|r| r["request"] == pending.id) {
        return Ok(false);
    }
    let record: Option<Value> = review::state::read(
        &root
            .join("native-binding-receipts")
            .join(format!("{}.json", pending.id)),
    )?;
    let Some(record) = record else {
        return Ok(false);
    };
    if record["request"] != pending.id
        || record["session"].as_str() != state.observer.session.as_deref()
        || record["pane"].as_str() != state.observer.pane.as_deref()
    {
        return Err(unbound("saved native receipt does not own pending report"));
    }
    let path = state
        .observer
        .transcript
        .as_ref()
        .ok_or_else(|| unbound("bound source missing"))?;
    let old: Proof = serde_json::from_value(record["proof"].clone())?;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let proof = verify(
        BufReader::new(file),
        &old.session,
        &state.observer.workspace,
        &old.marker,
    )?
    .ok_or_else(|| unbound("saved source no longer contains request receipt"))?;
    if proof.metadata_digest != old.metadata_digest || proof.receipt_digest != old.receipt_digest {
        return Err(unbound("saved native source bytes changed"));
    }
    persist(state, path, &proof)
}
