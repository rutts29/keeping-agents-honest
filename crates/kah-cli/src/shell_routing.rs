//! The shell convenience function preserves native administration commands.
use std::{os::unix::process::CommandExt, process::Command};

pub fn route(mut arguments: Vec<String>) -> crate::Result<Vec<String>> {
    if arguments.first().is_some_and(|s| s == "__codex") {
        arguments.remove(0);
        let help = Command::new("codex").arg("--help").output().ok();
        let help = help
            .as_ref()
            .filter(|o| o.status.success())
            .and_then(|o| std::str::from_utf8(&o.stdout).ok());
        if with_help(&arguments, help) {
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
    with_help(arguments, None)
}

pub fn with_help(arguments: &[String], help: Option<&str>) -> bool {
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
            command => {
                return !matches!(command, "resume" | "fork")
                    && (management_command(command) || listed_command(help, command));
            }
        }
    }
    false
}

fn listed_command(help: Option<&str>, command: &str) -> bool {
    help.and_then(|text| text.split_once("\nCommands:\n"))
        .map(|(_, section)| section.split("\n\n").next().unwrap_or(section))
        .is_some_and(|section| {
            section
                .lines()
                .any(|line| line.split_whitespace().next() == Some(command))
        })
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
