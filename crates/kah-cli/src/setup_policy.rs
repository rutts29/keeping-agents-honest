//! Explicit operator declarations, not secret or network classification heuristics.
use crate::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SetupPolicy {
    pub offline_mcp_servers: BTreeSet<String>,
    /// Declare secret-bearing servers false until the operator enables them once.
    pub secret_mcp_servers: BTreeMap<String, bool>,
}
impl SetupPolicy {
    pub fn load(home: &Path) -> Result<Self> {
        let bytes = match fs::read(home.join(".config/kah/setup.json")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("SETUP_CONFIG_READ stage=setup: {e}").into()),
        };
        serde_json::from_slice(&bytes).map_err(|e| {
            format!(
                "SETUP_CONFIG_JSON stage=setup: invalid JSON at line {}, column {}",
                e.line(),
                e.column()
            )
            .into()
        })
    }
}
