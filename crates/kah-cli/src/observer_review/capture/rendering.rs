//! Render every native field losslessly; annotate record ranges from trusted
//! JSON traversal, never from marker-looking text supplied by tools or users.
use crate::Result;
use kah_observer::review_evidence::{Document, NativeRecord};
use serde_json::{Value, json};

pub fn render(value: &Value) -> String {
    let mut text = String::new();
    walk(value, "native", &mut text);
    text
}
pub fn native_document(provider: &str, session: &str, value: &Value) -> Result<Document> {
    if !matches!(provider, "codex" | "claude") {
        return Err("REVIEW_PROVIDER stage=native_provenance".into());
    }
    let rows = value
        .as_array()
        .ok_or("REVIEW_NATIVE_RECORDS stage=native_provenance")?;
    let mut text = String::new();
    let mut records = vec![];
    for (index, row) in rows.iter().enumerate() {
        let start_byte = text.len();
        let path = format!("native/{index}");
        walk(row, &path, &mut text);
        records.push(NativeRecord {
            start_byte,
            end_byte: text.len(),
            context: context(provider, &path, row),
        });
    }
    let mut document = Document::new(
        "transcript",
        "worker_transcript",
        &format!("native:{provider}:{session}"),
        &text,
    );
    document.native_records = records;
    Ok(document)
}
fn context(provider: &str, path: &str, row: &Value) -> Value {
    let item = if provider == "codex" {
        &row["item"]
    } else {
        row
    };
    let content_types = item["message"]["content"]
        .as_array()
        .map(|a| a.iter().map(|v| v["type"].clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    json!({"provider":provider,"path":path,"native_transport_only":true,"record_type":item["type"],"record_id":if provider=="codex" {&item["id"]}else{&item["uuid"]},"turn_id":row["turnId"],"native_timestamp":row["timestamp"],"message_role":item["message"]["role"],"content_types":content_types,"is_meta":item["isMeta"],"is_sidechain":item["isSidechain"],"status":item["status"],"exit_code":item["exitCode"]})
}
fn walk(value: &Value, path: &str, out: &mut String) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                walk(value, &format!("{path}/{key}"), out);
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                walk(value, &format!("{path}/{index}"), out);
            }
        }
        Value::String(text) => out.push_str(&format!("\n[{path}]\n{text}\n")),
        other => out.push_str(&format!("\n[{path}]\n{other}\n")),
    }
}
