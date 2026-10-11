//! Lossless metadata versions in the existing digest-verified CAS, with deltas.
use super::{Result, fs};
use kah_cli::observer_review::source_store;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Default)]
pub struct Recording {
    previous: Map<String, Value>,
}
impl Recording {
    pub fn encode(&mut self, output: &Path, sample: &Value) -> Result<Value> {
        fs::create_dir_all(output.join("sources"))?;
        let fields = sample.as_object().ok_or("KEEP_UP_SAMPLE stage=record")?;
        let mut changed = Map::new();
        for (key, value) in fields {
            if self.previous.get(key) != Some(value) {
                changed.insert(key.clone(), references(output, value)?);
            }
        }
        let removed: Vec<_> = self
            .previous
            .keys()
            .filter(|k| !fields.contains_key(*k))
            .cloned()
            .collect();
        self.previous = fields.clone();
        Ok(json!({"format":"kah-keep-up-delta-v1","changed":changed,"removed":removed}))
    }
}
fn references(output: &Path, value: &Value) -> Result<Value> {
    match value {
        Value::Array(items) => items.iter().map(|v| references(output, v)).collect(),
        Value::Object(_) => {
            let text = serde_json::to_string(value)?;
            let digest = format!("{:x}", Sha256::digest(text.as_bytes()));
            let path = output.join("metadata").join(format!("{digest}.json"));
            if !path.exists() {
                fs::create_dir_all(path.parent().unwrap())?;
                source_store::text(output, &path, &text)?;
            }
            let mut reference: Value = serde_json::from_slice(&fs::read(path)?)?;
            reference["encoding"] = json!("json");
            Ok(reference)
        }
        _ => Ok(value.clone()),
    }
}
pub fn reconstruct(output: &Path, previous: &mut Value, delta: &Value) -> Result<Value> {
    if delta["format"] != "kah-keep-up-delta-v1" {
        return Err("KEEP_UP_FORMAT stage=reconstruct".into());
    }
    let previous = previous
        .as_object_mut()
        .ok_or("KEEP_UP_SAMPLE stage=reconstruct")?;
    for key in delta["removed"]
        .as_array()
        .ok_or("KEEP_UP_DELTA stage=reconstruct")?
    {
        previous.remove(key.as_str().ok_or("KEEP_UP_KEY stage=reconstruct")?);
    }
    for (key, value) in delta["changed"]
        .as_object()
        .ok_or("KEEP_UP_DELTA stage=reconstruct")?
    {
        previous.insert(
            key.clone(),
            source_store::expand(value.clone(), &output.join("sources"))?,
        );
    }
    Ok(Value::Object(previous.clone()))
}

/// Metrics need receipts, not repeated assessment bodies; full samples remain in CAS.
pub fn metrics_sample(sample: &Value) -> Value {
    let mut value = sample.clone();
    if let Some(reviews) = value["reviews"].as_array_mut() {
        for review in reviews {
            if let Some(fields) = review.as_object_mut() {
                fields.retain(|key, _| {
                    [
                        "id",
                        "head",
                        "review_class",
                        "commit_ms",
                        "at_ms",
                        "missing_stages",
                    ]
                    .contains(&key.as_str())
                });
            }
        }
    }
    value
}
