//! Catalog index stays in the packet; raw file contents share the existing CAS.
use super::*;
use crate::observer_review::{source_store, state::State};
use std::path::PathBuf;
pub fn persist(state: &State, documents: &mut [Document]) -> Result<()> {
    let Some(document) = documents.iter_mut().find(|d| d.id == "whole-tree-catalog") else {
        return Ok(());
    };
    let catalog: Value = serde_json::from_str(&document.text)?;
    let head = catalog["head"]
        .as_str()
        .ok_or("REVIEW_COMMIT_HEAD stage=packet")?;
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("REVIEW_COMMIT_HEAD stage=packet".into());
    }
    let root = crate::observer_review::state::Store::directory(&state.worker.session)?;
    let private = root.join("commit-trees");
    let public = state.observer.workspace.join("commit-trees");
    kah_core::run_store::ensure_private_directory(&private)?;
    kah_core::run_store::ensure_private_directory(&public)?;
    let name = format!("{head}.json");
    source_store::save(&private, &name, &json!({"state":catalog}))?;
    source_store::save(&public, &name, &json!({"state":catalog}))?;
    let index = catalog["files"]
        .as_array()
        .ok_or("REVIEW_COMMIT_TREE stage=packet")?
        .iter()
        .map(|d| json!({"id":d["id"],"uri":d["uri"],"digest":d["digest"]}))
        .collect::<Vec<_>>();
    let compact = json!({"head":head,"files":index,"full_source":public.join(name),"source_digest":format!("{:x}",Sha256::digest(serde_json::to_vec(&catalog)?))});
    *document = Document::new(
        &document.id,
        &document.kind,
        &document.uri,
        &serde_json::to_string(&compact)?,
    );
    source_store::share(&root, &state.observer.workspace)?;
    kah_run::access::share_tree(&public, kah_run::access::service_uid()?, false)?;
    Ok(())
}
pub(super) fn load(document: &Document) -> Result<Value> {
    let index: Value = serde_json::from_str(&document.text)?;
    let Some(path) = index["full_source"].as_str() else {
        return Ok(index);
    };
    let stored: Value = source_store::read(&PathBuf::from(path))?;
    let catalog = stored["state"].clone();
    let digest = format!("{:x}", Sha256::digest(serde_json::to_vec(&catalog)?));
    if index["source_digest"] != digest || index["head"] != catalog["head"] {
        return Err("REVIEW_SOURCE_DIGEST stage=packet".into());
    }
    Ok(catalog)
}
