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
    let table = config
        .entry("hooks")
        .or_insert_with(|| Value::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| failure("hooks is not a table"))?;
    append_declarations(table, hooks)?;
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())?;
    let mut client =
        Control::connect(native, directory, cwd, Duration::from_secs(10)).map_err(failure)?;
    let list = client
        .request("hooks/list", json!({"cwds":[cwd]}))
        .map_err(failure)?;
    let entries = list["data"][0]["hooks"]
        .as_array()
        .ok_or_else(|| failure("native hook list is absent"))?;
    let state = config["hooks"]
        .as_table_mut()
        .unwrap()
        .entry("state")
        .or_insert_with(|| Value::Table(Table::new()))
        .as_table_mut()
        .ok_or_else(|| failure("hook state is not a table"))?;
    for owned in hooks {
        let hook = entries
            .iter()
            .rev()
            .find(|h| {
                h["sourcePath"].as_str() == path.to_str()
                    && h["command"] == owned.command
                    && h["eventName"] == owned.api_event
            })
            .ok_or_else(|| failure("kah-owned hook was not discovered"))?;
        let key = hook["key"]
            .as_str()
            .ok_or_else(|| failure("hook has no key"))?;
        let hash = hook["currentHash"]
            .as_str()
            .ok_or_else(|| failure("hook has no native hash"))?;
        state.insert(
            key.into(),
            Value::try_from(json!({"enabled":true,"trusted_hash":hash})).map_err(failure)?,
        );
    }
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())
}

fn failure(error: impl ToString) -> OverlayError {
    OverlayError {
        code: "OWNED_HOOK_SETUP",
        detail: error.to_string(),
    }
}

fn append_declarations(table: &mut Table, hooks: &[OwnedHook<'_>]) -> Result<(), OverlayError> {
    for hook in hooks {
        let groups = table
            .entry(hook.event)
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .ok_or_else(|| failure("hook event is not an array"))?;
        let declaration =
            json!({"hooks":[{"type":"command","command":hook.command,"timeout":hook.timeout}]});
        groups.push(Value::try_from(declaration).map_err(failure)?);
    }
    Ok(())
}
