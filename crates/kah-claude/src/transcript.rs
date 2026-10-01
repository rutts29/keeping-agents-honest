//! Bounded JSONL intake. Native user envelopes alone do not imply human authorship.
use kah_core::{
    intent_history::{SourceProvider, SourceRole},
    observation::ObservationSnapshot,
    session_bundle::{BundleRecord, NeutralSessionBundle},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

pub const MAX_BYTES: usize = 32 * 1024 * 1024;

pub fn parse(text: &str, session: &str, project: &Path) -> Result<ObservationSnapshot, String> {
    parse_turn(text, session, project, None)
}

pub fn parse_turn(
    text: &str,
    session: &str,
    project: &Path,
    required_prompt: Option<&str>,
) -> Result<ObservationSnapshot, String> {
    if text.len() > MAX_BYTES {
        return Err("CLAUDE_SOURCE_BOUND stage=transcript: transcript exceeds 32 MiB".into());
    }
    let mut records = Vec::new();
    let mut seen = BTreeMap::new();
    let mut actions = Vec::new();
    let mut prompt_seen = required_prompt.is_none();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: Value = serde_json::from_str(line)
            .map_err(|_| "CLAUDE_SOURCE_JSON stage=transcript: invalid or incomplete JSONL")?;
        if row["type"] == "user"
            && row["sessionId"] == session
            && row["promptId"]
                .as_str()
                .is_some_and(|id| Some(id) == required_prompt)
        {
            prompt_seen = true;
        }
        match row["type"].as_str() {
            Some("user" | "attachment") => {
                let additions = user_records(&row, session, project)?;
                if additions.iter().any(BundleRecord::is_authoritative) {
                    actions.clear();
                }
                for record in additions.into_iter().filter(BundleRecord::is_authoritative) {
                    append(&mut records, &mut seen, record)?;
                }
                if row.get("toolUseResult").is_some() {
                    action(&mut actions, &row);
                }
            }
            Some("assistant") if row["isSidechain"] != true => action(&mut actions, &row),
            _ => (),
        }
    }
    if !prompt_seen {
        return Err(
            "CLAUDE_SOURCE_PENDING stage=transcript: current prompt is not durable yet".into(),
        );
    }
    snapshot(session, project, records, actions)
}

fn snapshot(
    session: &str,
    project: &Path,
    records: Vec<BundleRecord>,
    actions: Vec<Value>,
) -> Result<ObservationSnapshot, String> {
    let users = if records.is_empty() {
        None
    } else {
        Some(
            NeutralSessionBundle::from_records(
                SourceProvider::Claude,
                session,
                format!(
                    "sha256:{:x}",
                    Sha256::digest(project.as_os_str().as_encoded_bytes())
                ),
                records,
            )
            .map_err(|e| format!("CLAUDE_SOURCE_BUNDLE stage=transcript: {e}"))?,
        )
    };
    let action_digest = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&actions).unwrap())
    );
    Ok(ObservationSnapshot {
        users,
        actions,
        action_digest,
    })
}

fn user_records(row: &Value, session: &str, project: &Path) -> Result<Vec<BundleRecord>, String> {
    if row["type"] == "attachment" {
        return queued_records(row, session, project);
    }
    if row["isMeta"] == true
        || row["isVirtual"] == true
        || row["isVisibleInTranscriptOnly"] == true
        || row["isCompactSummary"] == true
        || row["isSidechain"] == true
        || row.get("toolUseResult").is_some()
        || row["message"]["content"]
            .as_array()
            .is_some_and(|parts| parts.iter().any(|p| p["type"] == "tool_result"))
        || row["message"]["role"] != "user"
    {
        return Ok(vec![]);
    }
    if row["sessionId"] != session {
        return Err("CLAUDE_SOURCE_SESSION stage=transcript: session mismatch".into());
    }
    if !row["cwd"]
        .as_str()
        .is_some_and(|cwd| Path::new(cwd).starts_with(project))
    {
        return Err("CLAUDE_SOURCE_PROJECT stage=transcript: cwd outside project".into());
    }
    let id = row["uuid"]
        .as_str()
        .ok_or("CLAUDE_SOURCE_ID stage=transcript: UUID absent")?;
    let content = &row["message"]["content"];
    let texts = match content {
        Value::String(text) => vec![(0, text.as_str())],
        Value::Array(parts) => parts
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                (p["type"] == "text")
                    .then(|| p["text"].as_str().map(|s| (i, s)))
                    .flatten()
            })
            .collect(),
        _ => return Err("CLAUDE_SOURCE_CONTENT stage=transcript: unsupported user content".into()),
    };
    texts
        .into_iter()
        .filter(|(_, t)| !t.trim().is_empty())
        .map(|(i, text)| {
            BundleRecord::new(format!("{id}:{i}"), role(row, text), text)
                .map_err(|e| format!("CLAUDE_SOURCE_RECORD stage=transcript: {e}"))
        })
        .collect()
}

fn queued_records(row: &Value, session: &str, project: &Path) -> Result<Vec<BundleRecord>, String> {
    let queued = &row["attachment"];
    if queued["type"] != "queued_command"
        || queued["commandMode"] != "prompt"
        || queued["origin"]["kind"] != "human"
        || queued["humanTurn"] != true
        || queued["isMeta"] == true
        || !queued["delivery_id"].is_string()
        || row
            .get("origin")
            .is_some_and(|origin| origin["kind"] != "human")
    {
        return Ok(vec![]);
    }
    // Claude 2.1.286 absorbs human input during a running turn as this native
    // attachment. Its rendered system reminder and queue-operation rows are
    // not authority. Use the original command UUID for cross-form deduplication.
    let mut user = row.clone();
    user["type"] = "user".into();
    user["uuid"] = queued["source_uuid"].clone();
    user["origin"] = queued["origin"].clone();
    user["message"] = json!({"role":"user","content":queued["prompt"]});
    user_records(&user, session, project)
}

fn role(row: &Value, text: &str) -> SourceRole {
    // An unknown origin is untrusted context, never inferred human authority.
    if row.get("origin").is_some() && row["origin"]["kind"] != "human" {
        return SourceRole::Delegate;
    }
    if row["turnOrigin"]
        .as_str()
        .is_some_and(|s| !matches!(s, "human" | "sdk" | "unknown"))
    {
        return SourceRole::SystemObservation;
    }
    let text = crate::paste::content_start(text)
        .trim_start()
        .to_ascii_lowercase();
    if ["[kah-reviewer]", "[kah-builder]", "[kah-relay]"]
        .iter()
        .any(|prefix| text.starts_with(prefix))
    {
        SourceRole::Delegate
    } else if text.starts_with("<system-reminder>")
        || text.starts_with("<local-command-")
        || text.starts_with("<command-name>")
        || text.starts_with("<task-notification>")
    {
        SourceRole::SystemObservation
    } else {
        SourceRole::User
    }
}

fn append(
    records: &mut Vec<BundleRecord>,
    seen: &mut BTreeMap<String, String>,
    record: BundleRecord,
) -> Result<(), String> {
    if let Some(digest) = seen.get(record.id()) {
        if digest != record.content_digest() {
            return Err("CLAUDE_SOURCE_REWRITE stage=transcript: UUID content changed".into());
        }
    } else {
        seen.insert(record.id().into(), record.content_digest().into());
        records.push(record);
    }
    Ok(())
}

fn action(actions: &mut Vec<Value>, row: &Value) {
    let text = row
        .get("toolUseResult")
        .unwrap_or(&row["message"])
        .to_string();
    let mut end = text.len().min(4096);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    actions.push(json!({"id":row["uuid"],"kind":row["type"],"evidence":&text[..end],"truncated":end < text.len()}));
    if actions.len() > 12 {
        actions.remove(0);
    }
}
