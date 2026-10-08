//! Select controller metadata before resolving payloads; evidence is unchanged.
use crate::Result;
use serde_json::Value;
use std::{collections::BTreeMap, fs::File, io::BufReader, path::Path};
mod visitor;
#[derive(Clone)]
pub enum Select {
    All,
    Raw,
    Fields(BTreeMap<String, Select>),
    Each(Box<Select>),
}
impl Select {
    pub fn fields(fields: &[(&str, Select)]) -> Self {
        Self::Fields(
            fields
                .iter()
                .map(|(key, value)| (key.to_string(), value.clone()))
                .collect(),
        )
    }
}
pub fn read_selected(path: &Path, selection: &Select) -> Result<Value> {
    let root = super::directory(path).unwrap_or_else(|_| path.parent().unwrap().join("sources"));
    let raw = visitor::parse(BufReader::new(File::open(path)?), selection)?;
    resolve(raw, &root, selection)
}
fn resolve(value: Value, root: &Path, selection: &Select) -> Result<Value> {
    if matches!(selection, Select::Raw) {
        return Ok(value);
    }
    if let Some(digest) = value.get("$kah_source").and_then(Value::as_str) {
        if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("REVIEW_SOURCE_ID stage=metadata".into());
        }
        let path = super::resolved(&root.join(digest), digest);
        if value["encoding"] == "json" {
            let raw = visitor::parse(super::segments::reader::open(&path, digest)?, selection)?;
            return resolve(raw, root, selection);
        }
        return if matches!(selection, Select::All) {
            super::expand(value, root)
        } else {
            Ok(Value::Null)
        };
    }
    match (value, selection) {
        (Value::Object(mut fields), Select::Fields(wanted)) => {
            let mut result = serde_json::Map::new();
            for (key, child) in wanted {
                if let Some(value) = fields.remove(key) {
                    result.insert(key.clone(), resolve(value, root, child)?);
                }
            }
            Ok(Value::Object(result))
        }
        (Value::Array(items), Select::Each(child)) => Ok(Value::Array(
            items
                .into_iter()
                .map(|value| resolve(value, root, child))
                .collect::<Result<_>>()?,
        )),
        (value, Select::All) => super::expand(value, root),
        (Value::Null, _) => Ok(Value::Null),
        _ => Ok(Value::Null),
    }
}
