//! Read provider-owned history. Children contribute usage, never user authority.
use crate::Result;
use kah_codex::{control::Control, overlay};
use kah_core::observation::ObservationSnapshot;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{BufReader, Read},
    path::{Path, PathBuf},
    time::Duration,
};

pub enum History {
    Codex {
        client: Control,
        project: PathBuf,
        session: String,
    },
    Claude {
        path: PathBuf,
        project: PathBuf,
        session: String,
    },
}
impl History {
    pub fn set_codex_name(&mut self, name: &str) -> Result<()> {
        let Self::Codex {
            client, session, ..
        } = self
        else {
            return Err("RUN_NAME_PROVIDER stage=run_history".into());
        };
        client.request("thread/name/set", json!({"threadId":session,"name":name}))?;
        Ok(())
    }
    pub fn completion_activity(&mut self) -> Result<Vec<Value>> {
        match self {
            Self::Codex {
                client, session, ..
            } => Ok(kah_codex::source::completion_activity(client, session)?),
            Self::Claude { .. } => Ok(self.snapshot()?.actions),
        }
    }
    pub fn completed(&mut self) -> Result<bool> {
        match self {
            Self::Codex {
                client, session, ..
            } => {
                let page = client.request("thread/turns/list", json!({"threadId":session,"limit":1,"sortDirection":"desc","itemsView":"notLoaded"}))?;
                Ok(page["data"]
                    .as_array()
                    .and_then(|turns| turns.first())
                    .is_some_and(|turn| turn["status"] == "completed"))
            }
            Self::Claude { path, session, .. } => Ok(kah_claude::lifecycle::completed(
                &claude_text(path)?,
                session,
            )?),
        }
    }
    pub fn open(
        home: &Path,
        directory: &Path,
        provider: &str,
        project: &Path,
        session: &str,
    ) -> Result<Self> {
        if provider == "claude" {
            if !session.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') || session.len() != 36 {
                return Err("RUN_CLAUDE_SESSION stage=run_history".into());
            }
            let path = home
                .join(".claude/projects")
                .join(project.to_string_lossy().replace('/', "-"))
                .join(format!("{session}.jsonl"));
            return Ok(Self::Claude {
                path,
                project: project.into(),
                session: session.into(),
            });
        }
        let inspection = directory.join("inspection");
        overlay::prepare(
            &home.join(".codex"),
            &inspection,
            &overlay::Policy {
                restricted: true,
                online: false,
                root: project,
                cwd: project,
            },
        )?;
        let mut client = Control::connect(
            &home.join(".local/bin/codex"),
            &inspection,
            project,
            Duration::from_secs(10),
        )?;
        let read = client.request(
            "thread/read",
            json!({"threadId":session,"includeTurns":false}),
        )?;
        if read["thread"]["cwd"].as_str() != project.to_str() || read["thread"]["id"] != session {
            return Err("RUN_HISTORY_SCOPE stage=run_history".into());
        }
        Ok(Self::Codex {
            client,
            project: project.into(),
            session: session.into(),
        })
    }
    pub fn snapshot(&mut self) -> Result<ObservationSnapshot> {
        match self {
            Self::Codex {
                client,
                project,
                session,
            } => Ok(kah_codex::source::observe(client, project, session)?),
            Self::Claude {
                path,
                project,
                session,
            } => Ok(kah_claude::transcript::parse(
                &claude_text(path)?,
                session,
                project,
            )?),
        }
    }
    pub fn tokens(&mut self) -> Result<u64> {
        match self {
            Self::Codex {
                client, session, ..
            } => {
                let read = client.request(
                    "thread/read",
                    json!({"threadId":session,"includeTurns":false}),
                )?;
                let mut files = rollout(&read["thread"])?.into_iter().collect();
                descendants(client, session, &mut files)?;
                files.iter().try_fold(0u64, |sum, path| {
                    let file = match fs::File::open(path) {
                        Ok(file) => file,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sum),
                        Err(e) => return Err(e.into()),
                    };
                    Ok(sum.saturating_add(kah_codex::usage::tokens(BufReader::new(file))?))
                })
            }
            Self::Claude { path, session, .. } => {
                let mut files = vec![path.clone()];
                claude_children(
                    &path.parent().unwrap().join(session).join("subagents"),
                    &mut files,
                )?;
                files.iter().try_fold(0u64, |sum, path| {
                    let file = match fs::File::open(path) {
                        Ok(file) => file,
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(sum),
                        Err(e) => return Err(e.into()),
                    };
                    Ok(sum.saturating_add(kah_claude::usage::tokens(BufReader::new(file))?))
                })
            }
        }
    }
}
fn claude_text(path: &Path) -> Result<String> {
    let mut text = String::new();
    fs::File::open(path)
        .and_then(|mut file| file.read_to_string(&mut text))
        .map_err(|e| format!("RUN_HISTORY_IO stage=run_history: {e}"))?;
    Ok(text)
}
fn rollout(thread: &Value) -> Result<Option<PathBuf>> {
    if thread.get("path") == Some(&Value::Null) {
        return Ok(None);
    }
    thread["path"]
        .as_str()
        .filter(|p| p.ends_with(".jsonl") && Path::new(p).is_absolute())
        .map(|s| Some(PathBuf::from(s)))
        .ok_or_else(|| "RUN_ROLLOUT_PATH stage=run_usage".into())
}
fn descendants(client: &mut Control, session: &str, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    for archived in [false, true] {
        let mut cursor = Value::Null;
        let mut seen = BTreeSet::new();
        for page_number in 0..100 {
            let page = client.request("thread/list", json!({"ancestorThreadId":session,"archived":archived,"limit":100,"cursor":cursor,"sourceKinds":["subAgent","subAgentReview","subAgentCompact","subAgentThreadSpawn","subAgentOther"]}))?;
            for thread in page["data"]
                .as_array()
                .ok_or("RUN_CHILDREN_SCHEMA stage=run_usage")?
            {
                if let Some(path) = rollout(thread)? {
                    files.insert(path);
                }
            }
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
            if page_number == 99 || !seen.insert(cursor.to_string()) {
                return Err("RUN_CHILDREN_BOUND stage=run_usage".into());
            }
        }
    }
    Ok(())
}
fn claude_children(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            claude_children(&entry.path(), files)?;
        } else if entry.file_type()?.is_file()
            && entry.path().extension().is_some_and(|s| s == "jsonl")
        {
            files.push(entry.path());
        }
    }
    Ok(())
}
