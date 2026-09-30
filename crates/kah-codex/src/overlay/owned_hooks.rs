use super::{OverlayError, storage::write_private};
use crate::control::Control;
use serde_json::json;
use std::{fs, path::Path, time::Duration};
use toml::{Table, Value};

pub struct OwnedHook<'a> {
    pub event: &'a str,
    pub api_event: &'a str,
    pub command: String,
    pub timeout: u64,
}

pub fn install_session_hook(
    native: &Path,
    directory: &Path,
    cwd: &Path,
    handler: &Path,
) -> Result<(), OverlayError> {
    let command = format!(
        "'{}' __herdr-session",
        handler.to_string_lossy().replace('\'', "'\\''")
    );
    install_owned_hooks(
        native,
        directory,
        cwd,
        &[OwnedHook {
            event: "SessionStart",
            api_event: "sessionStart",
            command,
            timeout: 2,
        }],
    )
}

/// Trust only exact kah-owned definitions discovered by the native provider.
/// Preserve existing user declarations/state; never blanket-bypass hook review.
pub fn install_owned_hooks(
    native: &Path,
    directory: &Path,
    cwd: &Path,
    hooks: &[OwnedHook<'_>],
) -> Result<(), OverlayError> {
    let path = directory.join("config.toml");
    let mut config: Table =
        toml::from_str(&fs::read_to_string(&path).map_err(failure)?).map_err(failure)?;
    config
        .entry("features")
        .or_insert_with(|| Value::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| failure("features is not a table"))?
        .insert("hooks".into(), Value::Boolean(true));
    let hook_path = directory.join("hooks.json");
    let mut file: serde_json::Value =
        serde_json::from_slice(&fs::read(&hook_path).map_err(failure)?).map_err(failure)?;
    append_declarations(
        file["hooks"]
            .as_object_mut()
            .ok_or_else(|| failure("hook events missing"))?,
        hooks,
    )?;
    write_private(
        &hook_path,
        &serde_json::to_vec_pretty(&file).map_err(failure)?,
    )?;
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())?;
    let mut client =
        Control::connect(native, directory, cwd, Duration::from_secs(10)).map_err(failure)?;
    let list = client
        .request("hooks/list", json!({"cwds":[cwd]}))
        .map_err(failure)?;
    let entries = list["data"][0]["hooks"]
        .as_array()
        .ok_or_else(|| failure("native hook list is absent"))?;
    let old = config
        .get("hooks")
        .and_then(Value::as_table)
        .and_then(|h| h.get("state"))
        .and_then(Value::as_table)
        .cloned()
        .unwrap_or_default();
    let state = trusted_entries(entries, &hook_path, hooks, &old)?;
    config.insert(
        "hooks".into(),
        Value::Table(Table::from_iter([("state".into(), Value::Table(state))])),
    );
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())
}

fn failure(error: impl ToString) -> OverlayError {
    OverlayError {
        code: "OWNED_HOOK_SETUP",
        detail: error.to_string(),
    }
}

fn append_declarations(
    table: &mut serde_json::Map<String, serde_json::Value>,
    hooks: &[OwnedHook<'_>],
) -> Result<(), OverlayError> {
    for hook in hooks {
        let groups = table
            .entry(hook.event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| failure("hook event is not an array"))?;
        groups.push(
            json!({"hooks":[{"type":"command","command":hook.command,"timeout":hook.timeout}]}),
        );
    }
    Ok(())
}
fn trusted_entries(
    entries: &[serde_json::Value],
    path: &Path,
    owned: &[OwnedHook<'_>],
    old: &Table,
) -> Result<Table, OverlayError> {
    let owned_keys = owned_keys(entries, path, owned)?;
    let mut state = Table::new();
    for entry in entries
        .iter()
        .filter(|e| e["sourcePath"].as_str() == path.to_str())
    {
        let key = entry["key"]
            .as_str()
            .ok_or_else(|| failure("hook has no key"))?;
        let hash = entry["currentHash"]
            .as_str()
            .ok_or_else(|| failure("hook has no native hash"))?;
        if owned_keys.contains(key) {
            state.insert(
                key.into(),
                Value::try_from(json!({"enabled":true,"trusted_hash":hash})).map_err(failure)?,
            );
        } else if let Some(Value::Table(previous)) = old.get(key) {
            let mut previous = previous.clone();
            if previous.get("trusted_hash").and_then(Value::as_str) != Some(hash) {
                previous.remove("trusted_hash");
            }
            state.insert(key.into(), Value::Table(previous));
        }
    }
    Ok(state)
}

fn owned_keys(
    entries: &[serde_json::Value],
    path: &Path,
    hooks: &[OwnedHook<'_>],
) -> Result<std::collections::BTreeSet<String>, OverlayError> {
    hooks
        .iter()
        .map(|hook| {
            entries
                .iter()
                .rev()
                .find(|entry| {
                    entry["sourcePath"].as_str() == path.to_str()
                        && entry["command"] == hook.command
                        && entry["eventName"] == hook.api_event
                })
                .and_then(|entry| entry["key"].as_str())
                .map(str::to_owned)
                .ok_or_else(|| failure("kah-owned hook was not discovered"))
        })
        .collect()
}

#[cfg(test)]
mod tests;
