//! Historical deferral anchors are evidence snapshots, not current authority.
use crate::{
    Result,
    observer_review::{source_store, state::Request},
};
use kah_observer::review_evidence::Document;
use serde_json::Value;
use std::{fs, path::Path};

/// Preserve the original snapshot and restore sources for this same request.
pub fn repair(worker: &str) -> Result<Value> {
    use crate::observer_review::{state::Store, workspace};
    let store = Store::open_wait(worker)?;
    let state = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=history_repair")?;
    let pending = state
        .pending
        .as_ref()
        .ok_or("REVIEW_PENDING_MISSING stage=history_repair")?;
    let mut request = workspace::request(&store, &pending.id)?;
    let path = store
        .directory
        .join("requests")
        .join(format!("{}.json", request.id));
    let archive = store.directory.join("context-before-history");
    kah_core::run_store::ensure_private_directory(&archive)?;
    fs::hard_link(
        &path,
        archive.join(format!(
            "{}-{}.json",
            request.id,
            crate::relay_store::nonce()
        )),
    )?;
    let before = request.documents.len();
    add(&store.directory, &mut request)?;
    source_store::save(
        path.parent().unwrap(),
        path.file_name().unwrap().to_str().unwrap(),
        &serde_json::to_value(&request)?,
    )?;
    let home = std::path::PathBuf::from(
        std::env::var_os("HOME").ok_or("REVIEW_HOME stage=history_repair")?,
    );
    workspace::prepare(&home, &state)?;
    workspace::publish(&store, &state, &request)?;
    crate::unattended_report::save(
        &store.directory,
        "gap.json",
        &serde_json::json!({
        "error":"REVIEW_CONTEXT_UPDATED stage=history_repair: original digest-verified deferral sources now have historical manifest aliases; cite the source ids in intent.json while preserving exact URI/quote/digest",
        "request":request.id,"at_ms":kah_observer::session::now_ms()}),
    )?;
    let receipt = serde_json::json!({"request":request.id,"historical_documents_added":request.documents.len()-before,
        "original_context_preserved":true,"same_observer_session":state.observer.session,"same_pane":state.observer.pane});
    drop(store);
    crate::observer_review::runtime::retry_pending(worker)?;
    Ok(receipt)
}

pub fn add(root: &Path, request: &mut Request) -> Result<()> {
    let Some(deferrals) = request.intent["open_deferrals"].as_array_mut() else {
        return Ok(());
    };
    for deferral in deferrals {
        let anchor = &deferral["requirement"];
        let (Some(source), Some(digest), Some(quote)) = (
            anchor["source"].as_str(),
            anchor["digest"].as_str(),
            anchor["quote"].as_str(),
        ) else {
            continue;
        };
        if request
            .documents
            .iter()
            .any(|d| d.id == source && d.digest == digest && d.text.contains(quote))
        {
            continue;
        }
        let original = deferral["introduced_at"]
            .as_str()
            .ok_or("REVIEW_DEFERRAL_HISTORY stage=context: missing original request")?;
        let uri = anchor["uri"].as_str().unwrap_or("");
        let mut document = original_document(root, original, source, digest, uri)?;
        if document.uri != anchor["uri"].as_str().unwrap_or("") || !document.text.contains(quote) {
            return Err(
                "REVIEW_DEFERRAL_HISTORY stage=context: original source does not verify its anchor"
                    .into(),
            );
        }
        let id = format!(
            "historical-{}",
            digest.strip_prefix("sha256:").unwrap_or(digest)
        );
        document.id = id.clone();
        document.kind = "historical_deferral_anchor".into();
        if !request.documents.iter().any(|d| d.id == id) {
            request.documents.push(document);
        }
        // URI, exact quote and original digest keep the controller identity;
        // only the manifest alias changes so it cannot collide with live ledger.
        deferral["requirement"]["source"] = id.into();
    }
    Ok(())
}

fn original_document(
    root: &Path,
    request: &str,
    source: &str,
    digest: &str,
    uri: &str,
) -> Result<Document> {
    if !request.starts_with("review-")
        || !request
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("REVIEW_REQUEST_ID stage=deferral_history".into());
    }
    let raw: Value = serde_json::from_slice(&fs::read(
        root.join("requests").join(format!("{request}.json")),
    )?)?;
    for stored in raw["documents"].as_array().into_iter().flatten() {
        // Retrieve whole archived documents; no invented source windows and
        // no current-ledger substitution for a historical declaration.
        let document: Document =
            serde_json::from_value(source_store::expand(stored.clone(), &root.join("sources"))?)?;
        if document.digest == digest
            && document.uri == uri
            && (document.id == source || source.starts_with("historical-"))
        {
            return Ok(document);
        }
    }
    Err(format!(
        "REVIEW_DEFERRAL_HISTORY stage=context: {request} lacks original source {source} {digest}"
    )
    .into())
}
