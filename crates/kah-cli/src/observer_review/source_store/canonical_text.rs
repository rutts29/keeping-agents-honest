//! A host job's canonical JSON is rendered only when read; citations stay exact.
use super::*;
use kah_observer::review_evidence::Document;

fn source(text: &str, root: &Path) -> Result<Option<Value>> {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return Ok(None);
    };
    if serde_json::to_string(&value)? != text {
        return Ok(None); // Preserve noncanonical spacing/escapes as ordinary text.
    }
    let mut stored = compact_source(&value, root)?;
    stored["encoding"] = json!("json_text");
    Ok(Some(stored))
}
pub(super) fn encode_document(value: &Value, root: &Path) -> Result<Option<Value>> {
    if value["id"] != "option1" || value["kind"] != "literal_first" {
        return Ok(None);
    }
    let Some(text) = value["text"].as_str() else {
        return Ok(None);
    };
    let Some(stored) = source(text, root)? else {
        return Ok(None);
    };
    let mut fields = serde_json::Map::new();
    for (key, value) in value.as_object().unwrap() {
        fields.insert(
            key.clone(),
            if key == "text" {
                stored.clone()
            } else {
                strings(value, root)?
            },
        );
    }
    Ok(Some(Value::Object(fields)))
}
pub(super) fn restore(bytes: &[u8], root: &Path) -> Result<String> {
    let value = expand(serde_json::from_slice(bytes)?, root)?;
    Ok(serde_json::to_string(&value)?)
}
pub fn publish(private: &Path, target: &Path, document: &Document) -> Result<()> {
    let root = private.join("sources");
    let stored = if document.id == "option1" && document.kind == "literal_first" {
        source(&document.text, &root)?
    } else {
        None
    };
    let stored = match stored {
        Some(stored) => stored,
        None => reference(&root, document.text.as_bytes(), "utf8")?,
    };
    crate::unattended_report::save(
        target.parent().unwrap(),
        target.file_name().unwrap().to_str().unwrap(),
        &stored,
    )
}
