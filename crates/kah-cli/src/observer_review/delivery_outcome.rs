//! A declined delivery is retained; it is never a report-correction loop.
use super::{
    source_store,
    state::{Control, State, Store},
};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
mod reader;
pub use reader::{Progress, scan};

#[derive(Serialize, Deserialize)]
struct Delivery {
    review: String,
    request: String,
    body_digest: String,
    observer: Option<String>,
    source: PathBuf,
    device: u64,
    inode: u64,
    progress: Progress,
}
pub fn reason(error: &dyn std::fmt::Display) -> bool {
    matches!(
        error.to_string().split_whitespace().next(),
        Some("DELIVERY_REFUSED" | "DELIVERY_UNANSWERED")
    )
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn begin(state: &State, request: &str, prompt: &str, offset: u64) -> Result<()> {
    let path = state
        .observer
        .transcript
        .as_ref()
        .ok_or("REVIEW_TRANSCRIPT_MISSING stage=delivery")?;
    let metadata = std::fs::metadata(path)?;
    let root = Store::directory(&state.worker.session)?.join("delivery-outcome");
    let _lock = Store::delivery_at(&root, true)?;
    let old: Option<Delivery> = super::state::read(&root.join("current.json"))?;
    let body_digest = digest(prompt.as_bytes());
    let progress = old
        .filter(|d| {
            d.request == request
                && d.body_digest == body_digest
                && d.observer == state.observer.session
                && d.source == *path
                && d.device == metadata.dev()
                && d.inode == metadata.ino()
                && d.progress.cursor <= metadata.len()
        })
        .map_or_else(
            || Progress {
                cursor: offset,
                ..Default::default()
            },
            |d| d.progress,
        );
    let delivery = Delivery {
        review: state
            .pending
            .as_ref()
            .ok_or("REVIEW_PENDING_MISSING stage=delivery")?
            .id
            .clone(),
        request: request.into(),
        body_digest,
        observer: state.observer.session.clone(),
        source: path.clone(),
        device: metadata.dev(),
        inode: metadata.ino(),
        progress,
    };
    source_store::save(&root, "current.json", &serde_json::to_value(delivery)?)
}

pub fn observe(worker: &str) -> Result<()> {
    let Some(state) = super::state::read_control(worker)? else {
        return Ok(());
    };
    let Some(pending) = &state.pending else {
        return Ok(());
    };
    let root = Store::directory(worker)?.join("delivery-outcome");
    let _lock = Store::delivery_at(&root, true)?;
    let Some(mut delivery): Option<Delivery> = super::state::read(&root.join("current.json"))?
    else {
        return Ok(());
    };
    if delivery.observer != state.observer.session || delivery.review != pending.id {
        return Ok(());
    }
    let metadata = std::fs::metadata(&delivery.source)?;
    if metadata.dev() != delivery.device
        || metadata.ino() != delivery.inode
        || metadata.len() < delivery.progress.cursor
    {
        return Err(
            "REVIEW_DELIVERY_SOURCE_CHANGED stage=delivery: native receipt source changed".into(),
        );
    }
    let saved: Option<Value> = super::state::read(&root.join("terminal.json"))?;
    if let Some(outcome) = saved.filter(|v| {
        v["request"] == delivery.request
            && v["body_digest"] == delivery.body_digest
            && v["observer"].as_str() == state.observer.session.as_deref()
    }) {
        return Err(format!(
            "{}: request {} retained; human decision required",
            outcome["error"]
                .as_str()
                .unwrap_or("DELIVERY_UNANSWERED stage=delivery"),
            delivery.request
        )
        .into());
    }
    if reported(&state, &delivery.request) {
        return Ok(());
    }
    let offset = delivery.progress.cursor;
    let outcome = scan(
        &state.worker.provider,
        state.observer.session.as_deref().unwrap_or(""),
        &delivery.source,
        &delivery.request,
        &delivery.body_digest,
        &mut delivery.progress,
    )?;
    if offset != delivery.progress.cursor {
        source_store::save(&root, "current.json", &serde_json::to_value(&delivery)?)?;
    }
    if let Some(outcome) = outcome {
        return retain(&state, &root, &delivery, outcome);
    }
    Ok(())
}
fn retain(state: &Control, root: &Path, delivery: &Delivery, mut outcome: Value) -> Result<()> {
    if outcome["exec_lost"] == true && replace(state, root, delivery, &outcome)? {
        return Ok(());
    }
    outcome["observer"] = json!(state.observer.session);
    outcome["source"] = json!(delivery.source);
    outcome["origin"] = json!(super::failure::Origin::Delivery);
    outcome["status"] = json!(if outcome["error"] == "DELIVERY_REFUSED stage=delivery" {
        "delivery_refused"
    } else {
        "delivery_unanswered"
    });
    source_store::save(
        &root.join("terminal"),
        &format!("{}.json", delivery.body_digest),
        &outcome,
    )?;
    source_store::save(root, "terminal.json", &outcome)?;
    source_store::save(
        &Store::directory(&state.worker.session)?,
        "gap.json",
        &outcome,
    )?;
    let text = format!(
        "[kah-observer-delivery-refused] Worker {} request {}: {}. Pending request and native reply retained; automatic redelivery stopped.",
        state.worker.session, delivery.request, outcome["error"]
    );
    super::provider_block::reviewer(state, &text, root)?;
    Err(format!(
        "{}: request {} retained",
        outcome["error"].as_str().unwrap(),
        delivery.request
    )
    .into())
}
/// The observer's exec transport can be lost during a daemon restart,
/// so the same observer process can never run the review. Once per request,
/// ask for a fresh observer process on the same native session and redeliver
/// the same request. Returns false when that was already done.
fn replace(state: &Control, root: &Path, delivery: &Delivery, outcome: &Value) -> Result<bool> {
    let once = root.join("exec-lost");
    if once.join(format!("{}.json", delivery.request)).is_file() {
        return Ok(false);
    }
    let record = json!({"request":delivery.request,"observer":state.observer.session,
        "native_record":outcome["native_record"],"at_ms":kah_observer::session::now_ms()});
    source_store::save(&once, &format!("{}.json", delivery.request), &record)?;
    let directory = Store::directory(&state.worker.session)?;
    source_store::save(&directory, REPLACE, &record)?;
    source_store::save(
        &directory,
        "gap.json",
        &json!({"error":"REVIEW_OBSERVER_EXEC_LOST stage=delivery: observer exec-server transport disconnected; replacing the observer process once",
            "request":delivery.request,"origin":super::failure::Origin::Delivery,"observer":state.observer.session}),
    )?;
    super::hook::spawn("__review-retry", &state.worker.session)?;
    Ok(true)
}
const REPLACE: &str = "observer-replace.json";
/// Whether `request` asks for a fresh observer process (see `replace`).
pub fn replacement_requested(directory: &Path, request: &str) -> Result<bool> {
    let marker: Option<Value> = super::state::read(&directory.join(REPLACE))?;
    Ok(marker.is_some_and(|m| m["request"] == request))
}
/// A fresh observer pane is being created; nothing is left to replace.
pub fn replacement_done(directory: &Path) -> Result<()> {
    match std::fs::remove_file(directory.join(REPLACE)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}
fn reported(state: &Control, request: &str) -> bool {
    let outbox = state.observer.workspace.join("outbox");
    [
        format!("submitted-{request}.json"),
        format!("selected-{request}.json"),
        format!("rejected-{request}.json"),
    ]
    .iter()
    .any(|name| outbox.join(name).is_file())
}
