//! Read public settings only, never credentials or the native global login file.
//! Configuration behavior inspected with Claude Code v2.1.296.
//! Settings reference: https://code.claude.com/docs/en/settings.
use std::path::Path;
#[path = "../terminal_support/public_config.rs"]
mod public_config;

pub fn snapshot(home: &Path) -> Option<Vec<u8>> {
    let path = kah_claude::paths::config_root(home).join("settings.json");
    public_config::snapshot(&path)
}
