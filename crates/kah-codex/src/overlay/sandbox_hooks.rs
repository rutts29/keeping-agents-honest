use super::{
    OverlayError,
    extensions::{failure, quote},
    storage::write_private,
    wrap_hook_commands,
};
use crate::control::Control;
use serde_json::{Value as Json, json};
use std::{collections::BTreeSet, fs, path::Path, time::Duration};
use toml::{Table, Value};

/// Carry approval only from unchanged, already-trusted user declarations. The
/// sandbox wrapper changes the native hash, not the user's hook body or state.
pub fn sandbox_hooks(
    native: &Path,
    directory: &Path,
    cwd: &Path,
    prefix: &[String],
) -> Result<(), OverlayError> {
    let hook_path = directory.join("hooks.json");
    let config_path = directory.join("config.toml");
    let mut config: Table =
        toml::from_str(&fs::read_to_string(&config_path).map_err(failure)?).map_err(failure)?;
    let old = states(&config);
    let before = list(native, directory, cwd)?;
    let approved = before
        .iter()
        .filter(|entry| entry["sourcePath"].as_str() == hook_path.to_str())
        .filter_map(|entry| {
            let key = entry["key"].as_str()?;
            (old.get(key)?.get("trusted_hash")?.as_str() == entry["currentHash"].as_str())
                .then(|| key.to_owned())
        })
        .collect::<BTreeSet<_>>();
    let mut hooks: Json =
        serde_json::from_slice(&fs::read(&hook_path).map_err(failure)?).map_err(failure)?;
    wrap_hook_commands(&mut hooks, prefix)?;
    write_private(
        &hook_path,
        &serde_json::to_vec_pretty(&hooks).map_err(failure)?,
    )?;
    let mut updated = old;
    for entry in list(native, directory, cwd)? {
        if let Some(key) = entry["key"].as_str().filter(|key| approved.contains(*key)) {
            let hash = entry["currentHash"]
                .as_str()
                .ok_or_else(|| failure("hook hash absent"))?;
            if let Some(table) = updated.get_mut(key).and_then(Value::as_table_mut) {
                table.insert("trusted_hash".into(), hash.into());
            }
        }
    }
    config.insert(
        "hooks".into(),
        Value::Table(Table::from_iter([("state".into(), Value::Table(updated))])),
    );
    wrap_notify(&mut config, prefix)?;
    write_private(
        &config_path,
        toml::to_string(&config).map_err(failure)?.as_bytes(),
    )
}
fn list(native: &Path, directory: &Path, cwd: &Path) -> Result<Vec<Json>, OverlayError> {
    let mut client =
        Control::connect(native, directory, cwd, Duration::from_secs(15)).map_err(failure)?;
    let list = client
        .request("hooks/list", json!({"cwds":[cwd]}))
        .map_err(failure)?;
    list["data"][0]["hooks"]
        .as_array()
        .cloned()
        .ok_or_else(|| failure("native hook list absent"))
}
fn states(config: &Table) -> Table {
    config
        .get("hooks")
        .and_then(Value::as_table)
        .and_then(|v| v.get("state"))
        .and_then(Value::as_table)
        .cloned()
        .unwrap_or_default()
}
fn wrap_notify(config: &mut Table, prefix: &[String]) -> Result<(), OverlayError> {
    let Some(notify) = config.get("notify").and_then(Value::as_array) else {
        return Ok(());
    };
    let command = notify
        .iter()
        .map(|v| {
            v.as_str()
                .map(quote)
                .ok_or_else(|| failure("notify arguments must be strings"))
        })
        .collect::<Result<Vec<_>, _>>()?
        .join(" ");
    // Native notify appends one JSON argument. Exec form keeps that argument intact.
    let mut args = prefix.to_vec();
    let Some(mode) = args.iter_mut().find(|s| s.as_str() == "--sandbox-shell") else {
        return Err(failure("shell prefix missing"));
    };
    *mode = "--sandbox-exec".into();
    args.extend([
        "/bin/sh".into(),
        "-c".into(),
        format!("exec {command} \"$@\""),
        "notify".into(),
    ]);
    config.insert(
        "notify".into(),
        Value::Array(args.into_iter().map(Value::String).collect()),
    );
    Ok(())
}
