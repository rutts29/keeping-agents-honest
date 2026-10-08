//! Whole native records / JSON values indexed without altering source evidence.
use super::{source_store, state::Request};
use crate::Result;
use kah_observer::review_evidence::Document;
use serde_json::{Value, json};
use std::path::Path;
mod prior;
pub fn publish(private: &Path, input: &Path, request: &Request) -> Result<Value> {
    let mut indices = vec![];
    for doc in &request.documents {
        if doc.kind == "lossless_reference" || !doc.native_records.is_empty() {
            let original = super::packet_references::original(doc)?;
            indices.push(publish_document(private, input, &original)?);
        }
    }
    let index = json!({"request":request.id,"sources":indices});
    std::fs::write(
        input.join("record-index.json"),
        serde_json::to_vec_pretty(&index)?,
    )?;
    Ok(index)
}
pub fn publish_document(private: &Path, input: &Path, document: &Document) -> Result<Value> {
    let directory = input.join("records").join(&document.id);
    std::fs::create_dir_all(&directory)?;
    let mut records = vec![];
    for (i, record) in document.native_records.iter().enumerate() {
        let text = document
            .text
            .get(record.start_byte..record.end_byte)
            .ok_or("REVIEW_RECORD_RANGE stage=index: native source annotation invalid")?;
        let path = directory.join(format!("{i}.txt"));
        source_store::text(private, &path, text)?;
        records.push(
            json!({"record":i,"path":path,"start_byte":record.start_byte,
            "end_byte":record.end_byte,"bytes":text.len(),"context":record.context,
            "source":document.id,"source_digest":document.digest}),
        );
    }
    if records.is_empty() && document.kind == "prior_review" {
        records = prior::publish(private, &directory, document)?;
    }
    Ok(
        json!({"source":document.id,"digest":document.digest,"uri":document.uri,
        "bytes":document.text.len(),"records":records}),
    )
}

pub fn publish_input(input: &Path, request: &Request) -> Result<()> {
    let documents: Vec<_> = request
        .documents
        .iter()
        .filter(|d| {
            !matches!(
                d.kind.as_str(),
                "controller_artifact_digests" | "lossless_tree_catalog"
            )
        })
        .map(|d| {
            json!({"id":d.id,"kind":d.kind,"digest":d.digest,
            "path":input.join(format!("{}.txt",d.id)),
            "in_packet":d.kind!="lossless_reference"})
        })
        .collect();
    let value = json!({"request":request.id,"head":request.head,
        "review_class":"commit","documents":documents,
        "record_index":input.join("record-index.json"),"full_manifest":input.join("manifest.json"),
        "intent":input.join("intent.json"),"schema":input.join("schema.json"),
        "note":"Full catalog/digests remain in full_manifest; no source was removed. Read record-index metadata to select complete relevant records; reuse local reads."});
    std::fs::write(
        input.join("light-input.json"),
        serde_json::to_vec_pretty(&value)?,
    )?;
    Ok(())
}
