use super::{fixture, journey::Worker};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

pub fn capture(command: Command, root: &Path, provider: &str) -> Command {
    let directory = root.join("proof-hooks");
    fs::create_dir_all(&directory).unwrap();
    let script = root.join(format!("capture-{provider}-hook.sh"));
    let quote = crate::pty::quote;
    fs::write(&script, format!("#!/bin/sh\nset -u\numask 077\ncapture=$(mktemp -d {}/{provider}.XXXXXX)\ncat > \"$capture/input.json\"\nstatus=0\n{} {} < \"$capture/input.json\" > \"$capture/output.json\" || status=$?\ncat \"$capture/output.json\"\nexit \"$status\"\n",
        quote(directory.to_str().unwrap()), quote(root.join("home/.local/bin/kah").to_str().unwrap()),
        if provider=="codex" {"__hook"} else {"__claude-hook"})).unwrap();
    fixture::executable(&script);
    if provider == "codex" {
        let home = command
            .get_envs()
            .find(|(key, _)| *key == "CODEX_HOME")
            .unwrap()
            .1
            .unwrap();
        let path = Path::new(home).join("hooks.json");
        let mut hooks: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        replace(&mut hooks, &script, false);
        fixture::save(&path, &hooks);
        return command;
    }
    let mut args = command
        .get_args()
        .map(|s| s.to_os_string())
        .collect::<Vec<_>>();
    let at = args.iter().position(|s| s == "--settings").unwrap() + 1;
    let mut settings: Value = serde_json::from_str(args[at].to_str().unwrap()).unwrap();
    replace(&mut settings, &script, true);
    args[at] = settings.to_string().into();
    let mut replacement = Command::new(command.get_program());
    replacement
        .args(args)
        .current_dir(command.get_current_dir().unwrap());
    for (key, value) in command.get_envs() {
        if let Some(value) = value {
            replacement.env(key, value);
        } else {
            replacement.env_remove(key);
        }
    }
    replacement
}
fn replace(value: &mut Value, script: &Path, claude: bool) {
    for (event, entries) in value["hooks"].as_object_mut().unwrap() {
        // Preserve the existing native SessionStart transcript-path recorder.
        if claude && event == "SessionStart" {
            continue;
        }
        for entry in entries.as_array_mut().unwrap() {
            for hook in entry["hooks"].as_array_mut().unwrap() {
                hook["command"] = if claude {
                    script.to_str().unwrap().into()
                } else {
                    crate::pty::quote(script.to_str().unwrap()).into()
                };
                if claude {
                    hook["args"] = json!([]);
                }
            }
        }
    }
}
pub fn nudge(worker: &Worker, text: &str) -> Option<Value> {
    for entry in fs::read_dir(worker.root.join("proof-hooks"))
        .ok()?
        .flatten()
    {
        let path = entry.path();
        let Ok(bytes) = fs::read(path.join("input.json")) else {
            continue;
        };
        let Ok(input) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if input["session_id"] != worker.id {
            continue;
        }
        let Ok(bytes) = fs::read(path.join("output.json")) else {
            continue;
        };
        let Ok(output) = serde_json::from_slice::<Value>(&bytes) else {
            continue;
        };
        if contains(&output, text) {
            return Some(json!({"path":path,"input":input,"output":output}));
        }
    }
    None
}
fn contains(value: &Value, text: &str) -> bool {
    match value {
        Value::String(s) => s.contains(text),
        Value::Array(a) => a.iter().any(|v| contains(v, text)),
        Value::Object(o) => o.values().any(|v| contains(v, text)),
        _ => false,
    }
}
