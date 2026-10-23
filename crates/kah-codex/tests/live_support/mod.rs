pub mod client;
pub mod extensions;
pub mod trust;

use client::Client;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// The operator's installed native Codex: `$HOME/.local/bin/codex`.
pub fn native_codex() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/bin/codex")
}
pub fn start(home: &Path, cwd: &Path, stderr: &Path) -> Client {
    let mut command = Command::new(native_codex());
    command
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", home)
        .env_remove("HERDR_ENV")
        .env_remove("CODEX_THREAD_ID")
        .current_dir(cwd);
    Client::start_command(command, stderr)
}
