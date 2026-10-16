//! Render every native field losslessly; annotate record ranges from trusted
//! JSON traversal, never from marker-looking text supplied by tools or users.
use crate::Result;
use kah_observer::review_evidence::{Document, NativeRecord};
use serde_json::{Value, json};

pub fn render(value: &Value) -> String {
    let mut text = String::new();
    walk(value, "native", &mut text, &[], &mut vec![]);
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
    let execution_contexts = super::executions::contexts(provider, rows);
    for (index, row) in rows.iter().enumerate() {
        let start_byte = text.len();
        let path = format!("native/{index}");
        let paths = output_paths(provider, &path, row);
        let mut outputs = vec![];
        walk(row, &path, &mut text, &paths, &mut outputs);
        let mut provenance = context(provider, &path, row);
        provenance["output_ranges"] = json!(outputs);
        provenance["executions"] = execution_contexts[index].clone();
        records.push(NativeRecord {
            start_byte,
            end_byte: text.len(),
            context: provenance,
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
    let mut context = json!({"provider":provider,"path":path,"native_transport_only":true,"record_type":item["type"],"record_id":if provider=="codex" {&item["id"]}else{&item["uuid"]},"turn_id":row["turnId"],"native_timestamp":row["timestamp"],"message_role":item["message"]["role"],"content_types":content_types,"is_meta":item["isMeta"],"is_sidechain":item["isSidechain"],"status":item["status"],"exit_code":item["exitCode"]});
    // Claude attachments carry their kind only here, e.g. queued_command for
    // input delivered during a running turn. Other records stay unchanged.
    if let Some(kind) = item["attachment"]["type"].as_str() {
        context["attachment_type"] = kind.into();
    }
    context
}
fn walk(
    value: &Value,
    path: &str,
    out: &mut String,
    expected: &[String],
    outputs: &mut Vec<[usize; 2]>,
) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                walk(value, &format!("{path}/{key}"), out, expected, outputs);
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                walk(value, &format!("{path}/{index}"), out, expected, outputs);
            }
        }
        Value::String(text) => {
            out.push_str(&format!("\n[{path}]\n"));
            let start = out.len();
            out.push_str(text);
            if expected.iter().any(|p| p == path) {
                outputs.push([start, out.len()]);
            }
            out.push('\n');
        }
        other => out.push_str(&format!("\n[{path}]\n{other}\n")),
    }
}

fn output_paths(provider: &str, base: &str, row: &Value) -> Vec<String> {
    if provider == "codex" {
        return if row["item"]["type"] == "commandExecution" {
            vec![format!("{base}/item/aggregatedOutput")]
        } else {
            vec![]
        };
    }
    if row["type"] != "user" {
        return vec![];
    }
    let mut paths = vec![];
    for (i, item) in row["message"]["content"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        if item["type"] != "tool_result" {
            continue;
        }
        paths.push(format!("{base}/message/content/{i}/content"));
        for (n, part) in item["content"].as_array().into_iter().flatten().enumerate() {
            if part["type"] == "text" {
                paths.push(format!("{base}/message/content/{i}/content/{n}/text"));
            }
        }
    }
    if !paths.is_empty() {
        paths.push(format!("{base}/toolUseResult/stdout"));
    }
    paths
}
