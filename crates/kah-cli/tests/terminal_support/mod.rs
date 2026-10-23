pub mod audit_runtime;
pub mod audit_scope;
pub mod closure;
pub mod herdr;
pub mod herdr_probe;
pub mod history;
pub mod processes;
pub mod pty;
pub mod startup;

/// The operator's native Codex login: restricted work must not see it.
pub fn host_codex_login() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var_os("HOME").unwrap()).join(".codex/auth.json")
}

/// That login path, quoted for a shell command.
pub fn host_codex_login_quoted() -> String {
    pty::quote(host_codex_login().to_str().unwrap())
}
