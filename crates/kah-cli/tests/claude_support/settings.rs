//! Read public settings only, never credentials or the native global login file.
//! anthropics/claude-code v2.1.296 release checked, then official settings docs
//! https://code.claude.com/docs/en/settings and installed config-root evidence:
//! Use the documented provider configuration-root contract.
use std::path::Path;
#[path = "../terminal_support/public_config.rs"]
mod public_config;

pub fn snapshot(home: &Path) -> Option<Vec<u8>> {
    let path = kah_claude::paths::config_root(home).join("settings.json");
    public_config::snapshot(&path)
}
