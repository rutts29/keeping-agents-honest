//! Fixture-only trace of the native submit-hook stdin; no provider config edits.
use std::{path::Path, process::Command};
pub fn command(original: &Command, root: &Path) -> Command {
    let mut args = original
        .get_args()
        .map(|s| s.to_os_string())
        .collect::<Vec<_>>();
    let at = args.iter().position(|s| s == "--settings").unwrap() + 1;
    let mut settings: serde_json::Value = serde_json::from_str(args[at].to_str().unwrap()).unwrap();
    let hook = &mut settings["hooks"]["UserPromptSubmit"][0]["hooks"][0];
    hook["command"] = "/bin/sh".into();
    hook["args"] = serde_json::json!([
        "-c",
        format!(
            "umask 077; tee {} | {} __claude-hook",
            crate::pty::quote(root.join("submit.json").to_str().unwrap()),
            crate::pty::quote(env!("CARGO_BIN_EXE_kah"))
        )
    ]);
    args[at] = settings.to_string().into();
    let mut command = Command::new(original.get_program());
    command
        .args(args)
        .current_dir(original.get_current_dir().unwrap());
    for (key, value) in original.get_envs() {
        if let Some(value) = value {
            command.env(key, value);
        } else {
            command.env_remove(key);
        }
    }
    command
}
