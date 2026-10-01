//! Per-launch policy rendered into an independent, persistent Codex home.
use std::{fmt, path::Path};
use toml::{Table, Value};

mod extensions;
mod sandbox_hooks;
pub use sandbox_hooks::sandbox_hooks;
mod hook_file;
mod owned_hooks;
mod storage;
pub use extensions::{route_mcp, wrap_hook_commands};
pub use owned_hooks::{OwnedHook, install_owned_hooks, install_session_hook};
pub use storage::prepare;

pub fn prepare_native(
    original: &Path,
    directory: &Path,
    policy: &Policy<'_>,
) -> Result<(), OverlayError> {
    // Native user sessions keep extensions; inspection homes remain minimal.
    prepare(
        original,
        directory,
        &Policy {
            restricted: false,
            online: policy.online,
            root: policy.root,
            cwd: policy.cwd,
        },
    )?;
    if policy.restricted {
        let mut projects = serde_json::Map::new();
        for path in [policy.root, policy.cwd] {
            projects.insert(
                path.to_string_lossy().into_owned(),
                serde_json::json!({"trust_level":"untrusted"}),
            );
        }
        write_settings(
            directory,
            serde_json::json!({"projects":projects,"web_search":"live"}),
        )?;
    }
    Ok(())
}

pub fn set_executor(directory: &Path, id: &str, args: &[String]) -> Result<(), OverlayError> {
    set_executors(directory, id, &[(id, args)])
}

pub fn set_executors(
    directory: &Path,
    default: &str,
    environments: &[(&str, &[String])],
) -> Result<(), OverlayError> {
    let ids = environments
        .iter()
        .map(|(id, _)| *id)
        .collect::<std::collections::BTreeSet<_>>();
    if !ids.contains(default) || ids.len() != environments.len() {
        return Err(OverlayError {
            code: "ENVIRONMENT_SELECTION",
            detail: "default must name one of the distinct executors".into(),
        });
    }
    let entries = environments
        .iter()
        .map(|(id, args)| {
            serde_json::json!({
                "id":id,"program":"/usr/bin/docker","args":args,"initialize_timeout_sec":30
            })
        })
        .collect::<Vec<_>>();
    let settings =
        serde_json::json!({"default":default,"include_local":false,"environments":entries});
    let text = toml::to_string(&settings).map_err(|e| OverlayError {
        code: "ENVIRONMENT_ENCODE",
        detail: e.to_string(),
    })?;
    storage::write_private(&directory.join("environments.toml"), text.as_bytes())
}

pub struct Policy<'a> {
    pub restricted: bool,
    pub online: bool,
    pub root: &'a Path,
    pub cwd: &'a Path,
}

#[derive(Debug)]
pub struct OverlayError {
    pub code: &'static str,
    pub detail: String,
}

impl fmt::Display for OverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} stage=codex_overlay: {}", self.code, self.detail)
    }
}
impl std::error::Error for OverlayError {}

pub fn render_config(source: &str, policy: &Policy<'_>) -> Result<String, OverlayError> {
    let mut config: Table = toml::from_str(source).map_err(|e| OverlayError {
        code: "CONFIG_PARSE",
        detail: e.to_string(),
    })?;
    if policy.restricted {
        restrict(&mut config, policy);
    }
    toml::to_string(&config).map_err(|e| OverlayError {
        code: "CONFIG_ENCODE",
        detail: e.to_string(),
    })
}

fn restrict(config: &mut Table, policy: &Policy<'_>) {
    // Preserve native model choice and presentation, not executable extension
    // configuration. An allowlist also excludes newly introduced host surfaces.
    const PREFERENCES: &[&str] = &[
        "model",
        "model_reasoning_effort",
        "model_reasoning_summary",
        "model_verbosity",
        "model_context_window",
        "model_auto_compact_token_limit",
        "service_tier",
        "personality",
        "model_provider",
        "model_providers",
        "preferred_auth_method",
        "cli_auth_credentials_store",
        "forced_login_method",
        "forced_chatgpt_workspace_id",
        "tui",
        "notice",
    ];
    config.retain(|key, _| PREFERENCES.contains(&key));
    config.insert("web_search".into(), Value::String("live".into()));
    let mut features = Table::new();
    for name in ["apps", "plugins", "remote_plugin", "hooks"] {
        features.insert(name.into(), Value::Boolean(false));
    }
    config.insert("features".into(), Value::Table(features));
    let mut projects = Table::new();
    for path in [policy.root, policy.cwd] {
        let mut settings = Table::new();
        settings.insert("trust_level".into(), Value::String("untrusted".into()));
        projects.insert(path.to_string_lossy().into_owned(), Value::Table(settings));
    }
    config.insert("projects".into(), Value::Table(projects));
}

/// Merge kah-owned settings into this independent home, never the shared config.
pub fn write_settings(directory: &Path, settings: serde_json::Value) -> Result<(), OverlayError> {
    let path = directory.join("config.toml");
    let mut config: Value =
        toml::from_str(&std::fs::read_to_string(&path).map_err(settings_failure)?)
            .map_err(settings_failure)?;
    merge_settings(
        &mut config,
        Value::try_from(settings).map_err(settings_failure)?,
    );
    storage::write_private(
        &path,
        toml::to_string(&config)
            .map_err(settings_failure)?
            .as_bytes(),
    )
}
fn merge_settings(target: &mut Value, incoming: Value) {
    if let (Some(target), Some(incoming)) = (target.as_table_mut(), incoming.as_table()) {
        for (key, value) in incoming {
            if let Some(existing) = target.get_mut(key) {
                merge_settings(existing, value.clone());
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = incoming;
    }
}

fn settings_failure(error: impl ToString) -> OverlayError {
    OverlayError {
        code: "OVERLAY_SETTINGS",
        detail: error.to_string(),
    }
}
