//! Private host receipts and native-hook endpoint registration.
use crate::Result;
use kah_core::{intent_history::SourceProvider, run_store::RunStore};
use kah_observer::session::{Binding, Store};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub provider: String,
    pub session: String,
    pub project: PathBuf,
    pub pane: Option<String>,
    pub socket: Option<PathBuf>,
}
impl Endpoint {
    pub fn source_provider(&self) -> Result<SourceProvider> {
        match self.provider.as_str() {
            "codex" => Ok(SourceProvider::Codex),
            "claude" => Ok(SourceProvider::Claude),
            _ => Err("RELAY_PROVIDER stage=relay".into()),
        }
    }
}
pub fn directory(session: &str) -> Result<PathBuf> {
    Ok(Store::directory(&RunStore::default_state_root()?, session))
}
pub fn endpoint(session: &str) -> Result<Endpoint> {
    let endpoint: Endpoint = read(&directory(session)?.join("endpoint.json"))?;
    let state = Store::read(&RunStore::default_state_root()?, session)?
        .ok_or("RELAY_SESSION stage=relay")?;
    if endpoint.session != session || endpoint.project != state.binding.project {
        return Err("RELAY_SCOPE stage=relay".into());
    }
    endpoint.source_provider()?;
    Ok(endpoint)
}
pub fn register(binding: &Binding, input: &Value, provider: &str) -> Result<()> {
    let path = directory(&binding.session)?;
    let (pane, socket) = if std::env::var("HERDR_ENV").as_deref() == Ok("1") {
        (
            std::env::var("HERDR_PANE_ID").ok(),
            std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from),
        )
    } else {
        (None, None)
    };
    let indexed = match (&pane, &socket) {
        (Some(pane), Some(socket)) => pane_path(pane, socket)?.exists(),
        _ => true,
    };
    if input["hook_event_name"] != "SessionStart" && path.join("endpoint.json").exists() && indexed
    {
        return Ok(());
    }
    let endpoint = Endpoint {
        provider: provider.into(),
        session: binding.session.clone(),
        project: binding.project.clone(),
        pane,
        socket,
    };
    crate::unattended_report::save(&path, "endpoint.json", &serde_json::to_value(&endpoint)?)?;
    if let (Some(pane), Some(socket)) = (&endpoint.pane, &endpoint.socket) {
        let index = pane_path(pane, socket)?;
        crate::unattended_report::save(
            index.parent().unwrap(),
            index.file_name().unwrap().to_str().unwrap(),
            &serde_json::json!({"session":binding.session}),
        )?;
    }
    Ok(())
}
/// The kah host hook, not Herdr's asynchronous process-discovery ID, owns binding.
pub fn pane_endpoint(pane: &str, socket: &Path) -> Result<Endpoint> {
    let value: Value = read(&pane_path(pane, socket)?)?;
    let endpoint = endpoint(
        value["session"]
            .as_str()
            .ok_or("RELAY_PANE_SESSION stage=relay")?,
    )?;
    if endpoint.pane.as_deref() != Some(pane) || endpoint.socket.as_deref() != Some(socket) {
        return Err("RELAY_PANE_SCOPE stage=relay".into());
    }
    Ok(endpoint)
}
pub fn pane_path(pane: &str, socket: &Path) -> Result<PathBuf> {
    let root = RunStore::default_state_root()?.join("relay-panes");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&root)?;
    let metadata = fs::symlink_metadata(&root)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("RELAY_PANE_PRIVATE stage=relay".into());
    }
    let key = serde_json::to_vec(&(socket, pane))?;
    Ok(root.join(format!("{:x}.json", Sha256::digest(key))))
}
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let metadata =
        fs::symlink_metadata(path).map_err(|e| format!("RELAY_READ stage=relay: {e}"))?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() > 256 * 1024
    {
        return Err("RELAY_PRIVATE stage=relay".into());
    }
    let bytes = fs::read(path).map_err(|e| format!("RELAY_READ stage=relay: {e}"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| "RELAY_JSON stage=relay: invalid private record".into())
}
pub fn receipt_name(id: &str) -> Result<String> {
    if id.is_empty()
        || id.len() > 128
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("RELAY_RECEIPT_ID stage=relay".into());
    }
    Ok(format!("relay-{id}.json"))
}
pub fn nonce() -> String {
    format!(
        "{:032x}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        std::process::id()
    )
}
