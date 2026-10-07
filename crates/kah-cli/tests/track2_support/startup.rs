use super::fixture::save;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

pub fn claude_trace(original: Command, root: &Path) -> Command {
    let mut args = original
        .get_args()
        .map(|s| s.to_os_string())
        .collect::<Vec<_>>();
    let at = args.iter().position(|s| s == "--settings").unwrap() + 1;
    let mut settings: Value = serde_json::from_str(args[at].to_str().unwrap()).unwrap();
    let handler = root.join("home/.local/bin/kah");
    let trace = root.join("claude-start.json");
    let hook = &mut settings["hooks"]["SessionStart"][0]["hooks"][0];
    hook["command"] = "/bin/sh".into();
    hook["args"] = json!([
        "-c",
        format!(
            "umask 077; tee {} | {} __claude-hook",
            crate::pty::quote(trace.to_str().unwrap()),
            crate::pty::quote(handler.to_str().unwrap())
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
fn transcript(provider: &str, id: &str, project: &Path) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let path = if provider == "claude" {
        let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
        let bytes = fs::read(root.join("claude-start.json")).ok()?;
        let input: Value = serde_json::from_slice(&bytes).ok()?;
        if input["session_id"] != id || input["cwd"] != project.to_str()? {
            return None;
        }
        PathBuf::from(input["transcript_path"].as_str()?)
    } else {
        find(&home.join(".codex/sessions"), id)?
    };
    assert!(
        path.starts_with(&home),
        "TRACK2_TRANSCRIPT_SCOPE stage=startup"
    );
    Some(path)
}
pub fn response(provider: &str, id: &str, project: &Path) -> Option<Value> {
    let path = transcript(provider, id, project)?;
    let mut current = false;
    let mut prompted = false;
    let mut identity = Value::Null;
    for line in BufReader::new(fs::File::open(&path).ok()?).lines() {
        let Ok(row) = serde_json::from_str::<Value>(&line.ok()?) else {
            continue;
        };
        let user = if provider == "codex" {
            row["type"] == "response_item" && row["payload"]["role"] == "user"
        } else {
            row["type"] == "user" && row["message"]["role"] == "user"
        };
        if user
            && row
                .to_string()
                .contains(&format!("Current private worktree: {}", project.display()))
        {
            prompted = true;
        }
        if provider == "codex"
            && row["type"] == "turn_context"
            && row["payload"]["cwd"] == project.to_str()?
        {
            identity = json!({"model":row["payload"]["model"],"effort":row["payload"]["effort"]});
            current = true;
        }
        let native = if provider == "codex" {
            prompted
                && current
                && row["type"] == "response_item"
                && row["payload"]["type"] == "message"
                && row["payload"]["role"] == "assistant"
                && row["payload"]["phase"] == "final_answer"
        } else {
            prompted
                && row["type"] == "assistant"
                && row["cwd"] == project.to_str()?
                && row["sessionId"] == id
        };
        if native {
            if provider == "claude" {
                identity = json!({"model":row["message"]["model"],"effort":row["effort"]});
            }
            assert_eq!(
                identity["model"],
                super::journey::model(provider),
                "TRACK2_MODEL_FLOOR stage=startup"
            );
            assert_eq!(identity["effort"], "low", "TRACK2_EFFORT stage=startup");
            return Some(identity_record(provider, id, project, &path, &identity));
        }
    }
    None
}
fn identity_record(
    provider: &str,
    id: &str,
    project: &Path,
    path: &Path,
    identity: &Value,
) -> Value {
    let result = json!({"provider":provider,"session":id,"project":project,"transcript":path,
        "bytes":fs::metadata(path).unwrap().len(),"identity":identity,"actual_assistant_response":true});
    let root = PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
    save(
        &root.join(format!("{provider}-native-identity.json")),
        &result,
    );
    result
}
fn find(root: &Path, id: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(root).ok()? {
        let entry = entry.ok()?;
        let path = entry.path();
        if entry.file_type().ok()?.is_dir() {
            if let Some(found) = find(&path, id) {
                return Some(found);
            }
        } else if path
            .file_name()?
            .to_str()?
            .ends_with(&format!("{id}.jsonl"))
        {
            return Some(path);
        }
    }
    None
}
