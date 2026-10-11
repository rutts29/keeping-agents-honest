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
