//! Native cosmetic names. Naming never supplies intent or session identity.
use kah_observer::session::{Binding, Store};
use serde_json::{Value, json};
use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt, path::Path, time::Duration};

fn mark(path: &Path) -> Result<(), String> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map(|_| ())
        .or_else(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                Ok(())
            } else {
                Err(format!("SESSION_NAME_IO stage=native_name: {e}"))
            }
        })
}
fn eligible(prompt: &str) -> bool {
    let prompt = prompt.trim();
    !prompt.is_empty()
        && !prompt.starts_with("[kah-builder]")
        && !prompt.starts_with("[kah-reviewer]")
}
fn title(session: &str, project: &Path, prompt: &str) -> String {
    let project = project
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    kah_run::native::session_name(&format!("{project}-{session}"), prompt)
}

pub fn claude_title(
    marker: &Path,
    session: &str,
    project: &Path,
    input: &Value,
) -> Result<Option<String>, String> {
    if marker.exists() {
        return Ok(None);
    }
    if input["session_title"]
        .as_str()
        .is_some_and(|s| !s.is_empty())
    {
        mark(marker)?;
        return Ok(None);
    }
    if input["hook_event_name"] != "UserPromptSubmit" {
        return Ok(None);
    }
    let Some(prompt) = input["prompt"].as_str().filter(|s| eligible(s)) else {
        return Ok(None);
    };
    let name = title(session, project, prompt);
    mark(marker)?;
    Ok(Some(name))
}

pub(crate) fn claude(
    base: &Path,
    binding: &Binding,
    input: &Value,
    output: &mut Value,
) -> Result<(), String> {
    if std::env::var_os("KAH_RUN_ID").is_some() {
        return Ok(());
    }
    let _lock = Store::open(base, &binding.session, Duration::from_secs(2))?;
    let marker = Store::directory(base, &binding.session).join("native-name");
    if let Some(name) = claude_title(&marker, &binding.session, &binding.project, input)? {
        if !output["hookSpecificOutput"].is_object() {
            output["hookSpecificOutput"] = json!({"hookEventName":input["hook_event_name"]});
        }
        output["hookSpecificOutput"]["sessionTitle"] = name.into();
    }
    Ok(())
}

pub(crate) fn codex(base: &Path, binding: &Binding, resume: bool) -> Result<(), String> {
    if std::env::var_os("KAH_RUN_ID").is_some() {
        return Ok(());
    }
    let store = Store::open(base, &binding.session, Duration::from_secs(2))?;
    let marker = Store::directory(base, &binding.session).join("native-name");
    if marker.exists() {
        return Ok(());
    }
    let Some(prompt) = store.sources()?.into_iter().find(|s| eligible(&s.text)) else {
        return Ok(());
    };
    let mut client = control(base, binding)?;
    let read = client
        .request(
            "thread/read",
            json!({"threadId":binding.session,"includeTurns":false}),
        )
        .map_err(|e| e.to_string())?;
    // Codex generates a title during the first turn. Replace that once, but
    // keep existing names when kah first encounters a resumed conversation.
    if !resume || read["thread"]["name"].as_str().is_none_or(str::is_empty) {
        client.request("thread/name/set", json!({"threadId":binding.session,"name":title(&binding.session, &binding.project, &prompt.text)})).map_err(|e| e.to_string())?;
    }
    mark(&marker)
}

fn control(base: &Path, binding: &Binding) -> Result<kah_codex::control::Control, String> {
    let directory = Store::directory(base, &binding.session).join("name-control");
    kah_codex::overlay::prepare(
        &binding.codex_home,
        &directory,
        &kah_codex::overlay::Policy {
            restricted: true,
            online: false,
            root: &binding.project,
            cwd: &binding.project,
        },
    )
    .map_err(|e| e.to_string())?;
    kah_codex::control::Control::connect(
        &binding.native,
        &directory,
        &binding.project,
        Duration::from_secs(10),
    )
    .map_err(|e| e.to_string())
}
