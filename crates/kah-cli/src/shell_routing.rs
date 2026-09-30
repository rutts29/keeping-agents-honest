//! The shell convenience function preserves native administration commands.
use std::{os::unix::process::CommandExt, process::Command};

pub fn route(mut arguments: Vec<String>) -> crate::Result<Vec<String>> {
    if arguments.first().is_some_and(|s| s == "__codex") {
        arguments.remove(0);
        if is_native_command(&arguments) {
            return Err(format!(
                "CODEX_EXEC stage=shell-routing: {}",
                Command::new("codex").args(&arguments).exec()
            )
            .into());
        }
        arguments.insert(0, "--".into());
    }
    Ok(arguments)
}

pub fn is_native_command(arguments: &[String]) -> bool {
    let mut args = arguments.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => return false,
            "--help" | "-h" | "--version" | "-V" => return true,
            "-c"
            | "--config"
            | "--enable"
            | "--disable"
            | "--remote"
            | "--remote-auth-token-env"
            | "-i"
            | "--image"
            | "-m"
            | "--model"
            | "--local-provider"
            | "-p"
            | "--profile"
            | "-s"
            | "--sandbox"
            | "-C"
            | "--cd"
            | "--add-dir"
            | "-a"
            | "--ask-for-approval" => {
                args.next();
            }
            value if value.starts_with('-') => (),
            command => return management_command(command),
        }
    }
    false
}

fn management_command(command: &str) -> bool {
    matches!(
        command,
        "agents"
            | "exec"
            | "e"
            | "review"
            | "login"
            | "logout"
            | "mcp"
            | "plugin"
            | "app-server"
            | "remote-control"
            | "completion"
            | "update"
            | "doctor"
            | "sandbox"
            | "debug"
            | "apply"
            | "a"
            | "queue"
            | "archive"
            | "delete"
            | "migrate-rollouts"
            | "unarchive"
            | "cloud"
            | "exec-server"
            | "features"
            | "help"
            | "execpolicy"
    )
}
