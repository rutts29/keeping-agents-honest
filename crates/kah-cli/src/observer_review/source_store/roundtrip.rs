//! Verify the stored representation against borrowed originals, one source at a
//! time. A storage buffer never selects, truncates or changes decision content.
use super::*;
use std::io::{BufReader, Read};

pub(super) fn matches(stored: &Value, expected: &Value, root: &Path) -> Result<bool> {
    if let Some(digest) = stored.get("$kah_source").and_then(Value::as_str) {
        if digest.len() != 64 || !digest.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err("REVIEW_SOURCE_ID stage=source_store".into());
        }
        let path = resolved(&root.join(digest), digest);
        let reader = segments::reader::open(&path, digest)?;
        return match stored["encoding"].as_str() {
            Some("binary") => {
                std::io::copy(&mut { reader }, &mut std::io::sink())?;
                Ok(stored == expected)
            }
            Some("utf8") => match expected.as_str() {
                Some(text) => equal_bytes(reader, text.as_bytes()),
                None => Ok(false),
            },
            Some("json") => {
                let value: Value = serde_json::from_reader(BufReader::new(reader))?;
                matches(&value, expected, root)
            }
            Some("json_text") => {
                let value: Value = serde_json::from_reader(BufReader::new(reader))?;
                let text = serde_json::to_string(&super::expand(value, root)?)?;
                Ok(expected.as_str() == Some(text.as_str()))
            }
            _ => Err(format!(
                "REVIEW_SOURCE_ENCODING stage=source_store: {}: unsupported encoding",
                path.display()
            )
            .into()),
        };
    }
    match (stored, expected) {
        (Value::Object(a), Value::Object(b)) if a.len() == b.len() => {
            for (key, value) in a {
                let Some(original) = b.get(key) else {
                    return Ok(false);
                };
                if !matches(value, original, root)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (value, original) in a.iter().zip(b) {
                if !matches(value, original, root)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(stored == expected),
    }
}
fn equal_bytes(mut reader: impl Read, expected: &[u8]) -> Result<bool> {
    let mut offset = 0;
    let mut buffer = [0u8; 8192];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            return Ok(offset == expected.len());
        }
        if expected.get(offset..offset + n) != Some(&buffer[..n]) {
            return Ok(false);
        }
        offset += n;
    }
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/source_roundtrip.rs"]
mod tests;
