use super::{OverlayError, storage::write_private};
use crate::control::Control;
use serde_json::json;
use std::{fs, path::Path, time::Duration};
use toml::{Table, Value};

/// Trust exactly the kah-owned declaration using Codex's own current hash. Never
/// bypass native hook review globally or trust a project/user hook by resemblance.
pub fn install_session_hook(
    native: &Path,
    directory: &Path,
    cwd: &Path,
    handler: &Path,
) -> Result<(), OverlayError> {
    let path = directory.join("config.toml");
    let text = fs::read_to_string(&path).map_err(failure)?;
    let mut config: Table = toml::from_str(&text).map_err(failure)?;
    let command = format!(
        "'{}' __herdr-session",
        handler.to_string_lossy().replace('\'', "'\\''")
    );
    let features = config
        .entry("features")
        .or_insert_with(|| Value::Table(Table::new()));
    features
        .as_table_mut()
        .ok_or_else(|| failure("features is not a table"))?
        .insert("hooks".into(), Value::Boolean(true));
    let declaration =
        json!({"SessionStart":[{"hooks":[{"type":"command","command":command,"timeout":2}]}]});
    config.insert(
        "hooks".into(),
        Value::try_from(declaration).map_err(failure)?,
    );
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())?;
    let mut client =
        Control::connect(native, directory, cwd, Duration::from_secs(10)).map_err(failure)?;
    let list = client
        .request("hooks/list", json!({"cwds":[cwd]}))
        .map_err(failure)?;
    let hook = list["data"][0]["hooks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| {
            h["sourcePath"].as_str() == path.to_str()
                && h["command"] == command
                && h["eventName"] == "sessionStart"
        })
        .ok_or_else(|| failure("kah's own session hook was not discovered"))?;
    let key = hook["key"]
        .as_str()
        .ok_or_else(|| failure("hook has no key"))?;
    let hash = hook["currentHash"]
        .as_str()
        .ok_or_else(|| failure("hook has no native hash"))?;
    let state = json!({key:{"enabled":true,"trusted_hash":hash}});
    config["hooks"]
        .as_table_mut()
        .unwrap()
        .insert("state".into(), Value::try_from(state).map_err(failure)?);
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())
}

fn failure(error: impl ToString) -> OverlayError {
    OverlayError {
        code: "OWNED_HOOK_SETUP",
        detail: error.to_string(),
    }
}
