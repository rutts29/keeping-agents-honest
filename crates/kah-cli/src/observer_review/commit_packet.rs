//! Diff scope changes default evidence, never the recoverable source content.
use super::state::{Purpose, Request};
use crate::Result;
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
mod read;
mod recovery;
mod storage;
pub use recovery::rejected as expand_rejected;
pub use storage::persist;
pub fn is_commit(request: &Request) -> bool {
    request.purpose == Purpose::Checkpoint && request.trigger["review_class"] == "commit"
}
pub fn is_scoped(request: &Request) -> bool {
    is_commit(request) || request.trigger["packet_scope"] == "diff-v1"
}
pub fn trigger(purpose: &Purpose, mut trigger: Value) -> Value {
    if matches!(purpose, Purpose::Drift | Purpose::Steering) || trigger["review_class"] == "commit"
    {
        trigger["packet_scope"] = json!("diff-v1");
    }
    if trigger["review_class"] == "commit" {
        trigger["review_protocol"] = json!("commit-v1");
    }
    trigger
}
pub fn reviewed(store: &super::state::Store, request: &Request) -> Result<()> {
    if request.purpose == Purpose::Checkpoint && request.head.is_some() {
        super::source_store::save(
            &store.directory,
            "last-reviewed-sha.json",
            &json!({"head":request.head,"request":request.id,"review_class":request.trigger["review_class"],"reviewed_not_necessarily_passed":true}),
        )?;
    }
    Ok(())
}
pub fn file_id(path: &str) -> String {
    format!("commit-file-{:x}", Sha256::digest(path.as_bytes()))
}
pub fn documents(project: &mut Value) -> Result<Vec<Document>> {
    let head = project["head"]
        .as_str()
        .ok_or("REVIEW_COMMIT_HEAD stage=packet")?;
    let context = &project["commit_context"];
    if context["head"] != head {
        return Err("REVIEW_COMMIT_HEAD_CHANGED stage=packet".into());
    }
    let tree = project["full_tree_files"]
        .as_array()
        .ok_or("REVIEW_COMMIT_TREE stage=packet")?;
    let files = tree
        .iter()
        .filter(|f| f.get("content").is_some())
        .map(|file| document(head, file))
        .collect::<Result<Vec<_>>>()?;
    let inventory = tree
        .iter()
        .map(|file| {
            let mut entry = file.clone();
            if entry.get("content").is_some() {
                entry.as_object_mut().unwrap().remove("content");
                entry["source"] = json!(file_id(file["path"].as_str().unwrap()));
            }
            entry
        })
        .collect::<Vec<_>>();
    let catalog = json!({"head":head,"files":files,"tree_entries":inventory,"snapshot":project["committed_snapshot"]});
    let mut docs = vec![Document::new(
        "whole-tree-catalog",
        "lossless_tree_catalog",
        &format!("git:{head}:complete-tree"),
        &serde_json::to_string(&catalog)?,
    )];
    docs.push(Document::new(
        "diff",
        "diff",
        "git:commit-diff",
        context["diff"]
            .as_str()
            .ok_or("REVIEW_COMMIT_DIFF stage=packet")?,
    ));
    touched(head, context, &mut docs)?;
    let object = project
        .as_object_mut()
        .ok_or("REVIEW_COMMIT_PROJECT stage=packet")?;
    object.remove("committed_snapshot");
    object.remove("full_tree_files");
    object.remove("commit_context");
    Ok(docs)
}
fn touched(head: &str, context: &Value, docs: &mut Vec<Document>) -> Result<()> {
    for file in context["touched_files"]
        .as_array()
        .ok_or("REVIEW_COMMIT_FILES stage=packet")?
    {
        if file["deleted"] == true {
            docs.push(Document::new(
                &file_id(
                    file["path"]
                        .as_str()
                        .ok_or("REVIEW_COMMIT_PATH stage=packet")?,
                ),
                "committed_deletion",
                &format!("git:{head}:deleted"),
                &serde_json::to_string(file)?,
            ));
        } else {
            docs.push(document(head, file)?);
        }
    }
    Ok(())
}
fn document(head: &str, file: &Value) -> Result<Document> {
    let path = file["path"]
        .as_str()
        .ok_or("REVIEW_COMMIT_PATH stage=packet")?;
    let text = match file["content"]["text"].as_str() {
        Some(text) => text.to_owned(),
        None => serde_json::to_string(&file["content"])?,
    };
    Ok(Document::new(
        &file_id(path),
        "committed_code",
        &format!("git:{head}:{path}"),
        &text,
    ))
}
pub fn validate_report(request: &Request, report: &Value) -> Result<()> {
    if !is_scoped(request) {
        return Ok(());
    }
    let mut sources = BTreeSet::new();
    collect_sources(report, &mut sources);
    for source in sources {
        if source == "whole-tree-catalog"
            || source.starts_with("commit-file-")
                && !request.documents.iter().any(|d| d.id == source)
        {
            return Err(format!("REVIEW_SOURCE_OUTSIDE_PACKET stage=report: host adds the complete cited file from whole-tree-catalog before retry ({source})").into());
        }
    }
    Ok(())
}
pub fn expand(request: &mut Request, report: &Value) -> Result<Vec<String>> {
    if !is_scoped(request) {
        return Ok(vec![]);
    }
    let Some(catalog) = request
        .documents
        .iter()
        .find(|d| d.id == "whole-tree-catalog")
    else {
        return Ok(vec![]);
    };
    if Document::new(&catalog.id, &catalog.kind, &catalog.uri, &catalog.text).digest
        != catalog.digest
    {
        return Err("REVIEW_SOURCE_DIGEST stage=packet".into());
    }
    let catalog = storage::load(catalog)?;
    if catalog["head"].as_str() != request.head.as_deref() {
        return Err("REVIEW_COMMIT_HEAD_CHANGED stage=packet".into());
    }
    let files: Vec<Document> = serde_json::from_value(catalog["files"].clone())?;
    let mut sources = BTreeSet::new();
    collect_sources(report, &mut sources);
    let mut added = vec![];
    for file in files {
        if sources.contains(&file.id) && !request.documents.iter().any(|d| d.id == file.id) {
            if Document::new(&file.id, &file.kind, &file.uri, &file.text).digest != file.digest {
                return Err("REVIEW_SOURCE_DIGEST stage=packet".into());
            }
            added.push(file.id.clone());
            request.documents.push(file);
        }
    }
    Ok(added)
}
fn collect_sources(value: &Value, sources: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            if let Some(source) = fields.get("source").and_then(Value::as_str) {
                sources.insert(source.into());
            }
            for value in fields.values() {
                collect_sources(value, sources);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_sources(value, sources);
            }
        }
        _ => (),
    }
}
