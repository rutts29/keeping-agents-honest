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
        if submitted_without_ack(&state)?
            || Store::directory(worker)?
                .join("native-binding-receipts")
                .join(format!("{}.json", state.pending.as_ref().unwrap().id))
                .is_file()
        {
            recheck_bound_pane(&state)?;
        }
        return continue_ready(worker, acknowledge_bound(&state)?);
    }
    let pane = state
        .observer
        .pane
        .as_deref()
        .ok_or_else(|| unbound("REVIEW_PANE_MISSING stage=observer"))?;
    let socket = state
        .worker
        .socket
        .as_deref()
        .ok_or_else(|| unbound("REVIEW_HERDR_REQUIRED stage=observer"))?;
    let view = crate::relay_transport::herdr(socket, &["pane", "get", pane])?;
    if let Some(phase) = starting(&state, &view["result"]["pane"]) {
        return Err(format!("{} stage=observer_binding: {phase}", super::STARTING).into());
    }
    review::control::owned_pane(&state).map_err(unbound)?;
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
fn submitted_without_ack(state: &State) -> Result<bool> {
    let Some(pending) = &state.pending else {
        return Ok(false);
    };
    let ready: Option<Value> =
        review::state::read(&Store::directory(&state.worker.session)?.join("report-ready.json"))?;
    Ok(!ready.is_some_and(|r| r["request"] == pending.id)
        && state
            .observer
            .workspace
            .join("outbox")
            .join(format!("submitted-{}.json", pending.id))
            .is_file())
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
fn session_mismatch(state: &State, pane: &Value) -> Option<Value> {
    let field = |field: &str, observed: &Value, expected: Value| {
        Some(json!({"field":field,"observed":observed,"expected":expected}))
    };
    let session = &pane["agent_session"];
    if pane["pane_id"].as_str() != state.observer.pane.as_deref() {
        return field("pane_id", &pane["pane_id"], json!(state.observer.pane));
    }
    if pane["agent"] != "codex" {
        return field("agent", &pane["agent"], json!("codex"));
    }
    if session["source"] != "herdr:codex" || session["kind"] != "id" {
        return field(
            "agent_session",
            session,
            json!({"source":"herdr:codex","kind":"id"}),
        );
    }
    pane["foreground_cwd"]
        .as_str()
        .is_none_or(|s| !review::control::contains_workspace(&state.observer.workspace, s))
        .then(|| json!({"field":"foreground_cwd","observed":pane["foreground_cwd"],"expected":state.observer.workspace}))
}
/// Before any native session is bound, the pane kah launched first shows its
/// shell (no agent yet), then Codex before Herdr publishes the session id.
/// Neither confers a binding and neither is a replacement. Any other identity,
/// a cwd outside the workspace or a changed process stays a mismatch.
pub(super) fn starting(state: &State, pane: &Value) -> Option<&'static str> {
    if state.observer.session.is_some()
        || pane["pane_id"].as_str() != state.observer.pane.as_deref()
        || pane["foreground_cwd"]
            .as_str()
            .is_none_or(|s| !review::control::contains_workspace(&state.observer.workspace, s))
        || state.observer.pid.is_some_and(|pid| {
            kah_observer::session::process_stamp(pid) != state.observer.process_stamp
        })
    {
        return None;
    }
    if pane["agent"].is_null() {
        return Some("native agent not yet detected");
    }
    (pane["agent"] == "codex" && pane["agent_session"].is_null())
        .then_some("native session not yet published")
}
pub(super) fn pane_session(state: &State, pane: &Value) -> Result<String> {
    if let Some(mismatch) = session_mismatch(state, pane) {
        review::control::record_mismatch(state, "pane_session", &mismatch);
        return Err(unbound(format!(
            "pane identity is not the owned native observer: {}",
            mismatch["field"].as_str().unwrap_or("identity")
        )));
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
pub(crate) fn locate(root: &Path, session: &str) -> Result<Option<PathBuf>> {
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
    proof_at(state, session, path)
}
fn proof_at(state: &State, session: &str, path: PathBuf) -> Result<(PathBuf, Proof)> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|e| unbound(format!("{}: {e}", path.display())))?;
    if !file.metadata()?.is_file() {
        return Err(unbound("native source is not a regular file"));
    }
    let decision = review::preparation::active_decision(
        &state.worker.session,
        &state.pending.as_ref().unwrap().id,
    )?;
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
        let session = state
            .observer
            .session
            .as_deref()
            .ok_or_else(|| unbound("bound native session missing"))?;
        let path = state
            .observer
            .transcript
            .clone()
            .ok_or_else(|| unbound("bound native source missing"))?;
        let (path, proof) = proof_at(state, session, path)?;
        return persist(state, &path, &proof);
    };
    let (path, proof) = saved_proof(state, &record)?;
    persist(state, &path, &proof)
}
fn saved_proof(state: &State, record: &Value) -> Result<(PathBuf, Proof)> {
    let pending = state
        .pending
        .as_ref()
        .ok_or_else(|| unbound("pending request missing"))?;
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
    Ok((path.clone(), proof))
}
