//! Native extension declarations, with execution location supplied by kah.
use super::{OverlayError, storage::write_private};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
use toml::{Table, Value};

pub fn route_mcp(
    directory: &Path,
    cwd: &Path,
    offline: &str,
    online: &str,
    offline_names: &BTreeSet<String>,
    secrets: &BTreeMap<String, bool>,
) -> Result<(), OverlayError> {
    let path = directory.join("config.toml");
    let mut config: Table =
        toml::from_str(&fs::read_to_string(&path).map_err(failure)?).map_err(failure)?;
    if let Some(servers) = config.get_mut("mcp_servers").and_then(Value::as_table_mut) {
        for (name, server) in servers {
            let server = server
                .as_table_mut()
                .ok_or_else(|| failure("MCP server must be a table"))?;
            if secrets.get(name) == Some(&false) {
                server.insert("enabled".into(), false.into());
            }
            if server.contains_key("command") {
                server.insert(
                    "environment_id".into(),
                    if offline_names.contains(name) {
                        offline
                    } else {
                        online
                    }
                    .into(),
                );
                server
                    .entry("cwd")
                    .or_insert_with(|| cwd.to_string_lossy().into_owned().into());
            }
        }
    }
    write_private(&path, toml::to_string(&config).map_err(failure)?.as_bytes())
}

pub fn wrap_hook_commands(
    hooks: &mut serde_json::Value,
    prefix: &[String],
) -> Result<(), OverlayError> {
    let events = hooks["hooks"]
        .as_object_mut()
        .ok_or_else(|| failure("hook events missing"))?;
    let prefix = prefix
        .iter()
        .map(|arg| quote(arg))
        .collect::<Vec<_>>()
        .join(" ");
    for groups in events.values_mut() {
        for group in groups
            .as_array_mut()
            .ok_or_else(|| failure("hook groups must be arrays"))?
        {
            for hook in group["hooks"]
                .as_array_mut()
                .ok_or_else(|| failure("hook handlers must be arrays"))?
            {
                if hook["type"] == "command" {
                    let command = hook["command"]
                        .as_str()
                        .ok_or_else(|| failure("command hook has no command"))?;
                    hook["command"] = format!("{prefix} {}", quote(command)).into();
                }
            }
        }
    }
    Ok(())
}

pub(super) fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
pub(super) fn failure(error: impl ToString) -> OverlayError {
    OverlayError {
        code: "EXTENSION_CONFIG",
        detail: error.to_string(),
    }
}
