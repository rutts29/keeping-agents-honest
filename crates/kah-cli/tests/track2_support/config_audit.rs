use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub fn snapshot(path: &Path, root: &Path) -> Value {
    let bytes = fs::read(path).unwrap();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    let mut ids = vec![root.to_str().unwrap().to_owned()];
    if let Ok(bytes) = fs::read(root.join("sessions.json")) {
        ids.extend(serde_json::from_slice::<Vec<String>>(&bytes).unwrap());
    }
    let mut mentions = BTreeMap::new();
    find(&value, "$", &ids, &mut mentions);
    json!({"sha256":format!("{:x}",Sha256::digest(&bytes)),"fixture_mentions":mentions})
}
fn find(value: &Value, path: &str, needles: &[String], found: &mut BTreeMap<String, String>) {
    let matched = |text: &str| needles.iter().any(|s| !s.is_empty() && text.contains(s));
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let next = format!("{path}/{key}");
                if matched(key) {
                    found.insert(
                        next.clone(),
                        format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap())),
                    );
                }
                find(value, &next, needles, found);
            }
        }
        Value::Array(values) => {
            for (n, value) in values.iter().enumerate() {
                find(value, &format!("{path}/{n}"), needles, found);
            }
        }
        Value::String(text) if matched(text) => {
            found.insert(
                path.into(),
                format!("{:x}", Sha256::digest(text.as_bytes())),
            );
        }
        _ => (),
    }
}
pub fn compare(before: &Value, after: &Value) -> Value {
    let passed = before["fixture_mentions"] == after["fixture_mentions"];
    json!({"passed":passed,"before":before,"after":after,"whole_file_changed":before["sha256"]!=after["sha256"],
        "attribution":if passed && before["sha256"]!=after["sha256"] {"changed by live sessions; not fixture-attributable"}
        else if passed {"unchanged"} else {"fixture-attributable key/project/trust entry changed"}})
}
