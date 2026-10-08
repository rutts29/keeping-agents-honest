//! Test-only Claude login boundary. Credential contents are never opened.
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::Path,
    process::Command,
};

pub const TEMPLATE_HOME: &str = "/mnt/kah-fixtures/cargo/claude-fixture-home";
pub const CONFIG_DIR: &str = "/mnt/kah-fixtures/cargo/claude-fixture-home/.claude";
pub const AUTH_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN",
    "CLAUDE_CODE_OAUTH_TOKEN_FILE",
    "CLAUDE_CODE_API_KEY",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

pub fn ready_at(home: &Path) -> Result<(), &'static str> {
    for directory in [home.to_path_buf(), home.join(".claude")] {
        let meta = fs::symlink_metadata(directory)
            .map_err(|_| "TRACK2_TEST_LOGIN_REQUIRED stage=login")?;
        if !meta.is_dir()
            || meta.uid()
                != kah_sandbox::current_uid().map_err(|_| "TRACK2_TEST_HOME_OWNER stage=login")?
            || meta.permissions().mode() & 0o777 != 0o700
        {
            return Err("TRACK2_TEST_HOME_PRIVATE stage=login");
        }
    }
    for file in [
        home.join(".claude/.credentials.json"),
        home.join("test-login-ready"),
    ] {
        let meta =
            fs::symlink_metadata(file).map_err(|_| "TRACK2_TEST_LOGIN_REQUIRED stage=login")?;
        if !meta.is_file()
            || meta.uid()
                != kah_sandbox::current_uid().map_err(|_| "TRACK2_TEST_HOME_OWNER stage=login")?
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err("TRACK2_TEST_LOGIN_PRIVATE stage=login");
        }
    }
    Ok(())
}
pub fn require_ready() {
    ready_at(Path::new(TEMPLATE_HOME))
        .expect("Claude tests stay paused until the separate interactive login succeeds");
}
pub fn command_environment(command: &mut Command) {
    for key in AUTH_ENV {
        command.env_remove(key);
    }
    command
        .env("DISABLE_AUTOUPDATER", "1")
        .env("CLAUDE_CONFIG_DIR", CONFIG_DIR);
}
pub fn requires_claude(mode: &str) -> bool {
    if mode == "cache" {
        return false;
    }
    if mode == "verify" {
        return std::env::var("KAH_GATE_REVIEW_PROVIDER").as_deref() == Ok("claude");
    }
    !(std::env::var_os("KAH_TRACK2_DRY_RUN").is_some()
        && std::env::var("KAH_TRACK2_DRY_PROVIDER").as_deref() == Ok("codex"))
}
