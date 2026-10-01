use super::Store;
use crate::intake::{Decision, Input};
use kah_core::intent_history::SourceRole;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
};
impl Store {
    /// Source history is separate from semantic projection and includes ignored input.
    pub fn sources(&self) -> Result<Vec<Input>, String> {
        let path = self.path.join("observations.jsonl");
        if fs::symlink_metadata(&path)
            .is_ok_and(|m| !m.is_file() || m.permissions().mode() & 0o077 != 0)
        {
            return Err("OBSERVER_SOURCE_PRIVATE stage=source_history".into());
        }
        let file = match fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(format!("OBSERVER_SOURCE_IO stage=source_history: {e}")),
        };
        let mut inputs = Vec::<Input>::new();
        let mut positions = BTreeMap::new();
        for line in BufReader::new(file).lines() {
            let value: Value =
                serde_json::from_str(&line.map_err(|_| "OBSERVER_SOURCE_IO stage=source_history")?)
                    .map_err(|_| "OBSERVER_SOURCE_JSON stage=source_history")?;
            match value["kind"].as_str() {
                Some("user_source_v1") => {
                    let input: Input = serde_json::from_value(value["input"].clone())
                        .map_err(|_| "OBSERVER_SOURCE_SCHEMA stage=source_history")?;
                    if input.source.role != SourceRole::User
                        || positions
                            .insert(input.key().to_owned(), inputs.len())
                            .is_some()
                    {
                        return Err("OBSERVER_SOURCE_AUTHORITY stage=source_history".into());
                    }
                    inputs.push(input);
                }
                Some("source_decision_v1") => {
                    let index = position(&positions, &value)?;
                    inputs[index].decision = Some(
                        serde_json::from_value(value["decision"].clone())
                            .map_err(|_| "OBSERVER_DECISION_SCHEMA stage=source_history")?,
                    );
                }
                Some("source_review_v1") => {
                    let index = position(&positions, &value)?;
                    inputs[index].dismissed = value["action"] == "dismiss";
                    inputs[index].promoted |= value["action"] == "promote";
                }
                _ => (),
            }
        }
        Ok(inputs)
    }
    pub fn add_source(&mut self, input: &Input) -> Result<(), String> {
        self.append(&json!({"kind":"user_source_v1","input":input}))
    }
    pub fn source_decision(&mut self, record: &str, decision: &Decision) -> Result<(), String> {
        self.append(&json!({"kind":"source_decision_v1","record":record,"decision":decision}))
    }
}
fn position(positions: &BTreeMap<String, usize>, value: &Value) -> Result<usize, String> {
    value["record"]
        .as_str()
        .and_then(|r| positions.get(r))
        .copied()
        .ok_or("OBSERVER_SOURCE_UNKNOWN stage=source_history".into())
}
