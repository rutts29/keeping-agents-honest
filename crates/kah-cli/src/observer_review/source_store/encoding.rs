use super::*;

pub(super) fn strings(value: &Value, root: &Path) -> Result<Value> {
    if value.get("$kah_source").is_some() {
        return Ok(value.clone());
    }
    match value {
        Value::String(text) => reference(root, text.as_bytes(), "utf8"),
        Value::Array(items) => Ok(Value::Array(
            items
                .iter()
                .map(|v| record(v, root))
                .collect::<Result<_>>()?,
        )),
        Value::Object(items) => Ok(Value::Object(
            items
                .iter()
                .map(|(k, v)| {
                    let stored = if k == "option1" && v.is_object() {
                        // The trigger and its text document share this exact job.
                        compact_source(v, root)?
                    } else {
                        strings(v, root)?
                    };
                    Ok((k.clone(), stored))
                })
                .collect::<Result<_>>()?,
        )),
        _ => Ok(value.clone()),
    }
}
fn record(value: &Value, root: &Path) -> Result<Value> {
    let encoded = strings(value, root)?;
    if value.is_object() && value.get("$kah_source").is_none() {
        // Whole native record identity and fields own one immutable CAS digest.
        reference(root, &serde_json::to_vec(&encoded)?, "json")
    } else {
        Ok(encoded)
    }
}
