//! A lossless context catalogue: source bodies stay in CAS, not the wake packet.
use super::state::Request;
use serde_json::{Value, json};
use std::path::Path;

pub fn view(request: &Request, input: &Path) -> Value {
    let sources = request
        .documents
        .iter()
        .map(|doc| {
            json!({"id":doc.id,"kind":doc.kind,"digest":doc.digest,
            "path":input.join(format!("{}.txt",doc.id)),"bytes":doc.text.len()})
        })
        .collect::<Vec<_>>();
    json!({"request":request.id,"purpose":request.purpose,"head":request.head,
        "sources":sources,"full_request":input.join("request.json"),
        "intent":input.join("intent.json"),"delta":input.join("delta.txt"),
        "schema":input.join("schema.json"),"record_index":input.join("record-index.json"),
        "storage":"Sources are complete digest-verified CAS references. Start with intent and delta; open whole relevant records/sources on demand. Never reconstruct every input into outbox copies. The canonical full request is for host validation, not a bulk context dump."})
}

/// Old pending requests need the catalogue too; decode only source metadata.
pub fn ensure(input: &Path) -> crate::Result<()> {
    use super::source_store::{Select, read_selected};
    let target = input.join("context-input.json");
    if target.exists() {
        return Ok(());
    }
    let doc = Select::fields(&[
        ("id", Select::All),
        ("kind", Select::All),
        ("digest", Select::All),
        ("uri", Select::All),
    ]);
    let meta = read_selected(
        &input.join("request.json"),
        &Select::fields(&[
            ("id", Select::All),
            ("purpose", Select::All),
            ("head", Select::All),
            ("documents", Select::Each(Box::new(doc))),
        ]),
    )?;
    let sizes = read_selected(
        &input.join("manifest.json"),
        &Select::fields(&[(
            "documents",
            Select::Each(Box::new(Select::fields(&[
                ("id", Select::All),
                ("bytes", Select::All),
            ]))),
        )]),
    )?;
    let documents = meta["documents"]
        .as_array()
        .ok_or("REVIEW_CONTEXT_CATALOGUE stage=context_input")?;
    let sources=documents.iter().map(|d| {
        let size=sizes["documents"].as_array().and_then(|rows|rows.iter().find(|row|row["id"]==d["id"])).map(|row|row["bytes"].clone()).unwrap_or(Value::Null);
        json!({"id":d["id"],"kind":d["kind"],"digest":d["digest"],"path":input.join(format!("{}.txt",d["id"].as_str().unwrap_or(""))),"bytes":size})
    }).collect::<Vec<_>>();
    let catalogue = json!({"request":meta["id"],"purpose":meta["purpose"],"head":meta["head"],"sources":sources,
        "full_request":input.join("request.json"),"intent":input.join("intent.json"),"delta":input.join("delta.txt"),"schema":input.join("schema.json"),"record_index":input.join("record-index.json"),
        "storage":"Complete CAS sources; read intent/delta and relevant whole sources on demand. Never materialize the full canonical request or every source into per-request scratch."});
    crate::unattended_report::save(input, "context-input.json", &catalogue)
}
