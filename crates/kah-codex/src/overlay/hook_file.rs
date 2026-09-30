//! Merge only user-global declarations; restricted homes get no user hooks.
use super::{OverlayError, storage::write_private};
use serde_json::{Map, Value, json};
use std::{fs, path::Path};
use toml::Table;

pub(super) fn prepare(
    original: &Path,
    directory: &Path,
    config: &mut Table,
    restricted: bool,
) -> Result<(), OverlayError> {
    let path = directory.join("hooks.json");
    let old = config
        .remove("hooks")
        .and_then(|v| v.as_table().cloned())
        .unwrap_or_default();
    let state = old
        .get("state")
        .and_then(|v| v.as_table())
        .cloned()
        .unwrap_or_default();
    let mut events = Map::new();
    let mut mapped = Table::new();
    if !restricted {
        let source = global_events(original)?;
        merge(
            &original.join("hooks.json"),
            &source,
            &state,
            &path,
            &mut events,
            &mut mapped,
        )?;
        let declarations = old
            .into_iter()
            .filter(|(k, _)| k != "state")
            .map(|(k, v)| serde_json::to_value(v).map(|v| (k, v)))
            .collect::<Result<Map<_, _>, _>>()
            .map_err(failure)?;
        merge(
            &original.join("config.toml"),
            &declarations,
            &state,
            &path,
            &mut events,
            &mut mapped,
        )?;
    }
    if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        if fs::read_link(&path).map_err(failure)? != original.join("hooks.json") {
            return Err(failure("unexpected hooks symlink"));
        }
        fs::remove_file(&path).map_err(failure)?;
    }
    write_private(
        &path,
        &serde_json::to_vec_pretty(&json!({"hooks":events})).map_err(failure)?,
    )?;
    config.insert(
        "hooks".into(),
        toml::Value::Table(Table::from_iter([(
            "state".into(),
            toml::Value::Table(mapped),
        )])),
    );
    Ok(())
}
fn global_events(original: &Path) -> Result<Map<String, Value>, OverlayError> {
    let bytes = match fs::read(original.join("hooks.json")) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(e) => return Err(failure(e)),
    };
    let input: Value = serde_json::from_slice(&bytes).map_err(failure)?;
    input
        .get("hooks")
        .and_then(Value::as_object)
        .cloned()
        .ok_or_else(|| failure("hooks.json must contain a hooks object"))
}
fn merge(
    source: &Path,
    input: &Map<String, Value>,
    state: &Table,
    target: &Path,
    output: &mut Map<String, Value>,
    mapped: &mut Table,
) -> Result<(), OverlayError> {
    for (event, groups) in input {
        let groups = groups
            .as_array()
            .ok_or_else(|| failure("hook event must be an array"))?;
        let destination = output
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .unwrap();
        let offset = destination.len();
        let snake = snake_case(event);
        for (group, definition) in groups.iter().enumerate() {
            let handlers = definition["hooks"]
                .as_array()
                .ok_or_else(|| failure("hook handlers must be an array"))?;
            for handler in 0..handlers.len() {
                let old = format!("{}:{snake}:{group}:{handler}", source.display());
                if let Some(value) = state.get(&old) {
                    let new = format!("{}:{snake}:{}:{handler}", target.display(), offset + group);
                    mapped.insert(new, value.clone());
                }
            }
        }
        destination.extend(groups.iter().cloned());
    }
    Ok(())
}
fn snake_case(event: &str) -> String {
    let mut result = String::new();
    for (i, c) in event.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_ascii_lowercase());
    }
    result
}
pub(super) fn failure(e: impl ToString) -> OverlayError {
    OverlayError {
        code: "OVERLAY_HOOK_FILE",
        detail: e.to_string(),
    }
}
