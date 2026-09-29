pub mod client;
pub mod extensions;
pub mod trust;

use client::Client;
use std::{path::Path, process::Command};
pub fn start(home: &Path, cwd: &Path, stderr: &Path) -> Client {
    let mut command = Command::new("/home/example/.local/bin/codex");
    command
        .args(["app-server", "--stdio"])
        .env("CODEX_HOME", home)
        .env_remove("HERDR_ENV")
        .env_remove("CODEX_THREAD_ID")
        .current_dir(cwd);
    Client::start_command(command, stderr)
}
