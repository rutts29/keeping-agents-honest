//! Bounded native history pages for the automatic observer. Mutable tool output
//! is evidence, not part of the append-only cursor for authoritative user text.
use super::*;
use crate::control::{Control, ControlError};

pub use kah_core::observation::ObservationSnapshot;

pub fn observe(
    client: &mut Control,
    root: &Path,
    id: &str,
) -> Result<ObservationSnapshot, SourceError> {
    let id = validate_explicit_session_id(id)?;
    let root = root.canonicalize().map_err(|e| {
        SourceError::new("CODEX_OBSERVER_ROOT", "source_observation", e.to_string())
    })?;
    let root = root.to_str().ok_or_else(|| {
        SourceError::new(
            "CODEX_OBSERVER_ROOT",
            "source_observation",
            "project is not UTF-8",
        )
    })?;
    let mut response = client
        .request("thread/read", json!({"threadId":id,"includeTurns":false}))
        .map_err(rpc_error)?;
    let entries = read_items(client, &id)?;
    let mut turns = Vec::<Value>::new();
    for entry in &entries {
        let turn = required_string(entry, "turnId", "thread/items/list")?;
        if turns.last().is_none_or(|last| last["id"] != turn) {
            turns.push(json!({"id":turn,"items":[]}));
        }
        turns.last_mut().unwrap()["items"]
            .as_array_mut()
            .unwrap()
            .push(entry["item"].clone());
    }
    response["thread"]["turns"] = json!(turns);
    let goal = match client.request("thread/goal/get", json!({"threadId":id})) {
        Ok(value) => parse_native_goal(value, &id)?,
        Err(e) if e.code == "CODEX_RPC_REJECTED" => NativeGoal::Unknown,
        Err(e) => return Err(rpc_error(e)),
    };
    let imported = parse_imported_session(response, root, &id, goal.clone(), false, None)?;
    let users = user_bundle(&imported)?;
    let actions = action_evidence(&entries);
    let action_digest = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&actions).expect("values serialize"))
    );
    Ok(ObservationSnapshot {
        users,
        actions,
        action_digest,
    })
}

fn read_items(client: &mut Control, id: &str) -> Result<Vec<Value>, SourceError> {
    let mut result = Vec::new();
    let mut cursor = Value::Null;
    let mut seen = BTreeSet::new();
    for _ in 0..1000 {
        let page = match client.request(
            "thread/items/list",
            json!({"threadId":id,"cursor":cursor,"limit":25,"sortDirection":"asc"}),
        ) {
            Ok(page) => page,
            Err(e) if e.code == "CODEX_RPC_REJECTED" && e.detail.contains("-32601") => {
                return legacy_items(client, id);
            }
            Err(e) => return Err(rpc_error(e)),
        };
        let items = required_array(&page, "data", "thread/items/list")?;
        result.extend(items.iter().cloned());
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            return Ok(result);
        }
        if !seen.insert(cursor.to_string()) {
            return Err(SourceError::new(
                "CODEX_OBSERVER_CURSOR",
                "source_observation",
                "native item cursor repeated",
            ));
        }
    }
    Err(SourceError::new(
        "CODEX_OBSERVER_LIMIT",
        "source_observation",
        "native history exceeds the page bound",
    ))
}

fn action_evidence(entries: &[Value]) -> Vec<Value> {
    let last_turn = entries.last().map(|e| &e["turnId"]);
    let mut actions = entries
        .iter()
        .rev()
        .filter(|e| Some(&e["turnId"]) == last_turn)
        .filter_map(|entry| bounded_action(&entry["item"]))
        .take(12)
        .collect::<Vec<_>>();
    actions.reverse();
    actions
}

fn bounded_action(item: &Value) -> Option<Value> {
    let content = match item["type"].as_str()? {
        "agentMessage" => json!({"text": item["text"]}),
        "commandExecution" => json!({
            "command": item["command"], "cwd": item["cwd"],
            "status": item["status"], "exitCode": item["exitCode"],
            "output": item["aggregatedOutput"]
        }),
        "fileChange" => json!({"changes": item["changes"], "status": item["status"]}),
        _ => return None,
    };
    let text = content.to_string();
    let mut end = text.len().min(4096);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(json!({"id": item["id"], "kind": item["type"],
        "exit_code":item["exitCode"], "status":item["status"],
        "evidence": &text[..end], "truncated": end < text.len()}))
}

fn rpc_error(error: ControlError) -> SourceError {
    SourceError::new(error.code, "codex_control", error.detail)
}

pub(super) fn user_bundle(
    imported: &ImportedSession,
) -> Result<Option<NeutralSessionBundle>, SourceError> {
    let users = imported
        .records
        .iter()
        .filter(|r| r.role == SourceRole::User)
        .map(|r| BundleRecord::new(&r.id, r.role.clone(), &r.content))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            SourceError::new(
                "CODEX_OBSERVER_USER_RECORD",
                "source_observation",
                e.to_string(),
            )
        })?;
    let users = if users.is_empty() {
        None
    } else {
        Some(
            NeutralSessionBundle::from_records_with_native_goal(
                SourceProvider::Codex,
                &imported.session_id,
                &imported.repository_digest,
                imported.native_goal.clone(),
                users,
            )
            .map_err(|e| {
                SourceError::new("CODEX_OBSERVER_BUNDLE", "source_observation", e.to_string())
            })?,
        )
    };
    Ok(users)
}

/// Redacted provenance audit of native user-role transport records; no text leaves this function.
pub fn provenance_audit(client: &mut Control, id: &str) -> Result<Value, SourceError> {
    let entries = read_items(client, &validate_explicit_session_id(id)?)?;
    let mut records = Vec::new();
    for entry in entries {
        let item = &entry["item"];
        if item["type"] != "userMessage" {
            continue;
        }
        let mut imported = Vec::new();
        parse_user_message(item, "provenance_audit", &mut imported)?;
        for record in imported {
            let source = match record.role {
                SourceRole::User => "user",
                SourceRole::Delegate => "delegate",
                _ => "system",
            };
            records.push(json!({"record_hash":format!("{:x}",Sha256::digest(record.id.as_bytes())),"source":source,"client_id":item["clientId"]}));
        }
    }
    Ok(
        json!({"records":records,"scope":"Native user-role records classified by explicit source markers. Bare terminal paste has no sender identity in Codex thread/read."}),
    )
}

fn legacy_items(client: &mut Control, id: &str) -> Result<Vec<Value>, SourceError> {
    let read = client
        .request("thread/read", json!({"threadId":id,"includeTurns":true}))
        .map_err(rpc_error)?;
    let turns = required_array(&read["thread"], "turns", "legacy thread/read")?;
    let mut items = vec![];
    for turn in turns {
        let id = required_string(turn, "id", "legacy turn")?;
        for item in required_array(turn, "items", "legacy turn")? {
            items.push(json!({"turnId":id,"item":item}));
        }
    }
    Ok(items)
}
