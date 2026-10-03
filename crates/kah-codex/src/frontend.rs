//! Credential-free native TUI construction. Not a daemon launcher or fallback.
use std::{fs, path::Path, process::Command};

type Result<T> = std::result::Result<T, String>;
fn io(error: impl std::fmt::Display) -> String {
    format!("CODEX_CLIENT_IO stage=codex_client: {error}")
}
fn credential_free(home: &Path) -> Result<()> {
    match fs::symlink_metadata(home.join("auth.json")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io(e)),
        Ok(_) => Err("CODEX_CLIENT_CREDENTIALS_PRESENT stage=codex_client: client homes must contain no auth.json, including symlinks".into()),
    }
}

pub fn prepare(login_home: &Path, client_home: &Path) -> Result<()> {
    if client_home == login_home || client_home.starts_with(login_home) {
        return Err(
            "CODEX_CLIENT_HOME stage=codex_client: use a separate kah-owned client home".into(),
        );
    }
    if fs::symlink_metadata(client_home).is_ok_and(|m| !m.is_dir()) {
        return Err("CODEX_CLIENT_HOME stage=codex_client: directory alias refused".into());
    }
    credential_free(client_home)?;
    kah_core::run_store::ensure_private_directory(client_home).map_err(io)?;
    if client_home.canonicalize().map_err(io)? == login_home.canonicalize().map_err(io)? {
        return Err("CODEX_CLIENT_HOME stage=codex_client: client cannot be the login home".into());
    }
    let text = fs::read_to_string(login_home.join("config.toml")).map_err(io)?;
    let source: toml::Table = toml::from_str(&text)
        .map_err(|_| "CODEX_CLIENT_CONFIG stage=codex_client: cannot parse native preferences")?;
    let mut config = toml::Table::new();
    // Execution, extensions and login belong to the server. Copy only native
    // presentation/model preferences; no MCP/tool secrets enter client homes.
    for key in [
        "model",
        "model_reasoning_effort",
        "model_reasoning_summary",
        "model_verbosity",
        "service_tier",
        "personality",
        "tui",
        "notice",
    ] {
        if let Some(value) = source.get(key) {
            config.insert(key.into(), value.clone());
        }
    }
    config.insert("cli_auth_credentials_store".into(), "ephemeral".into());
    crate::overlay::storage::write_private(
        &client_home.join("config.toml"),
        toml::to_string(&config).map_err(io)?.as_bytes(),
    )
    .map_err(io)?;
    Ok(())
}

pub fn command(
    binary: &Path,
    client_home: &Path,
    socket: &Path,
    cwd: &Path,
    native: &[String],
) -> Result<Command> {
    credential_free(client_home)?;
    if !socket.is_absolute() || !socket.exists() {
        return Err("CODEX_DAEMON_REQUIRED stage=codex_client: shared daemon is absent; embedded fallback is forbidden".into());
    }
    let split = native
        .iter()
        .position(|a| a == "--")
        .unwrap_or(native.len());
    if native[..split]
        .iter()
        .any(|a| a == "--no-daemon" || a == "--remote" || a.starts_with("--remote="))
    {
        return Err(
            "CODEX_AUTH_OWNER_OVERRIDE stage=codex_client: kah selects the sole daemon endpoint"
                .into(),
        );
    }
    let mut command = Command::new(binary);
    command
        .current_dir(cwd)
        .env("CODEX_HOME", client_home)
        .env_remove("CODEX_EXEC_SERVER_URL")
        .env_remove("OPENAI_API_KEY")
        .env_remove("CODEX_API_KEY")
        .env_remove("CODEX_ACCESS_TOKEN")
        .args(["--remote", &format!("unix://{}", socket.display())])
        .args(&native[..split])
        // Last config override wins, before any literal prompt delimiter.
        .args(["-c", "cli_auth_credentials_store=\"ephemeral\""])
        .args(&native[split..]);
    Ok(command)
}
