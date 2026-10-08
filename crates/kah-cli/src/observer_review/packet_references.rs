//! Lossless on-demand originals, never a content limit or authority summary.
use super::{commit_packet, source_store, state::Request};
use crate::Result;
use kah_observer::review_evidence::Document;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reference {
    full_source: PathBuf,
    source_digest: String,
    original_digest: String,
    original_kind: String,
}
pub fn scope(private: &Path, workspace: &Path, request: &mut Request) -> Result<()> {
    if !commit_packet::is_scoped(request) {
        return Ok(());
    }
    let archive = private.join("packet-originals");
    let public = workspace.join("packet-originals");
    kah_core::run_store::ensure_private_directory(&archive)?;
    kah_core::run_store::ensure_private_directory(&public)?;
    for document in &mut request.documents {
        if !matches!(
            document.kind.as_str(),
            "worker_transcript" | "prior_review" | "historical_deferral_anchor"
        ) && !document.id.starts_with("historical-")
        {
            continue;
        }
        let original = serde_json::to_value(&*document)?;
        let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&original)?));
        let name = format!("{digest}.json");
        source_store::save(&archive, &name, &json!({"state":original}))?;
        source_store::save(&public, &name, &json!({"state":original}))?;
        let reference = Reference {
            full_source: public.join(name),
            source_digest: digest,
            original_digest: document.digest.clone(),
            original_kind: document.kind.clone(),
        };
        *document = Document::new(
            &document.id,
            "lossless_reference",
            &document.uri,
            &serde_json::to_string(&reference)?,
        );
    }
    source_store::share(private, workspace)?;
    kah_run::access::share_tree(&public, kah_run::access::service_uid()?, false)?;
    Ok(())
}
pub fn outside(request: &Request, source: &str) -> bool {
    request
        .documents
        .iter()
        .any(|d| d.id == source && d.kind == "lossless_reference")
}
pub fn expand(
    request: &mut Request,
    sources: &std::collections::BTreeSet<String>,
) -> Result<Vec<String>> {
    let mut added = vec![];
    for document in &mut request.documents {
        if document.kind != "lossless_reference" || !sources.contains(&document.id) {
            continue;
        }
        if Document::new(&document.id, &document.kind, &document.uri, &document.text).digest
            != document.digest
        {
            return Err("REVIEW_SOURCE_DIGEST stage=packet_reference".into());
        }
        let reference: Reference = serde_json::from_str(&document.text)?;
        let stored: Value = source_store::read(&reference.full_source)?;
        if format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&stored["state"])?)
        ) != reference.source_digest
        {
            return Err("REVIEW_SOURCE_DIGEST stage=packet_reference".into());
        }
        let original: Document = serde_json::from_value(stored["state"].clone())?;
        if original.id != document.id
            || original.uri != document.uri
            || original.digest != reference.original_digest
            || original.kind != reference.original_kind
            || Document::new(&original.id, &original.kind, &original.uri, &original.text).digest
                != original.digest
        {
            return Err("REVIEW_SOURCE_DIGEST stage=packet_reference".into());
        }
        added.push(original.id.clone());
        *document = original;
    }
    Ok(added)
}
