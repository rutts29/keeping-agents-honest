use super::*;

pub(super) fn parse_user_message(
    item: &Value,
    item_location: &str,
    records: &mut Vec<ImportedRecord>,
) -> Result<(), SourceError> {
    let id = required_string(item, "id", item_location)?;
    let content = required_array(item, "content", item_location)?;
    for (content_index, part) in content.iter().enumerate() {
        let part_location = format!("{item_location}.content[{content_index}]");
        let provider_type = required_string(part, "type", &part_location)?;
        let (role, content) = if provider_type == "text" {
            let content = required_text(part, "text", &part_location)?;
            let role = if crate::plugin_control::parse(&content).is_some() {
                SourceRole::SystemObservation
            } else {
                SourceRole::User
            };
            (role, content)
        } else {
            validate_user_attachment(part, &provider_type, &part_location)?;
            (
                SourceRole::SystemObservation,
                opaque_provider_evidence(part, &format!("userInput.{provider_type}"))?,
            )
        };
        records.push(ImportedRecord {
            id: format!("{id}:{content_index}"),
            role,
            content,
        });
    }
    Ok(())
}

pub(super) fn provider_item_role(provider_type: &str) -> Option<SourceRole> {
    match provider_type {
        "agentMessage" | "plan" | "reasoning" => Some(SourceRole::Assistant),
        "commandExecution"
        | "fileChange"
        | "mcpToolCall"
        | "dynamicToolCall"
        | "collabAgentToolCall"
        | "webSearch"
        | "imageView"
        | "sleep"
        | "imageGeneration" => Some(SourceRole::Tool),
        "hookPrompt" | "subAgentActivity" | "enteredReviewMode" | "exitedReviewMode"
        | "contextCompaction" => Some(SourceRole::SystemObservation),
        _ => None,
    }
}

pub(super) fn validate_provider_item(
    item: &Value,
    provider_type: &str,
    location: &str,
) -> Result<(), SourceError> {
    required_string(item, "id", location)?;
    match provider_type {
        "agentMessage" | "plan" => {
            required_provider_text(item, "text", location)?;
        }
        "reasoning" | "contextCompaction" => {}
        "hookPrompt" => {
            required_array(item, "fragments", location)?;
        }
        "commandExecution" => {
            required_provider_text(item, "command", location)?;
            required_array(item, "commandActions", location)?;
            required_string(item, "cwd", location)?;
            required_string(item, "status", location)?;
        }
        "fileChange" => {
            required_array(item, "changes", location)?;
            required_string(item, "status", location)?;
        }
        "mcpToolCall" => {
            required_field(item, "arguments", location)?;
            required_string(item, "server", location)?;
            required_string(item, "tool", location)?;
            required_string(item, "status", location)?;
        }
        "dynamicToolCall" => {
            required_field(item, "arguments", location)?;
            required_string(item, "tool", location)?;
            required_string(item, "status", location)?;
        }
        "collabAgentToolCall" => validate_collaboration_item(item, location)?,
        "subAgentActivity" => {
            required_string(item, "agentPath", location)?;
            required_string(item, "agentThreadId", location)?;
            required_string(item, "kind", location)?;
        }
        "webSearch" => {
            required_provider_text(item, "query", location)?;
        }
        "imageView" => {
            required_string(item, "path", location)?;
        }
        "sleep" => {
            required_u64(item, "durationMs", location)?;
        }
        "imageGeneration" => {
            required_provider_text(item, "result", location)?;
            required_string(item, "status", location)?;
        }
        "enteredReviewMode" | "exitedReviewMode" => {
            required_provider_text(item, "review", location)?;
        }
        _ => unreachable!("provider role validation precedes item validation"),
    }
    Ok(())
}

pub(super) fn validate_user_attachment(
    part: &Value,
    provider_type: &str,
    location: &str,
) -> Result<(), SourceError> {
    match provider_type {
        "image" | "audio" => {
            required_provider_text(part, "url", location)?;
        }
        "localImage" | "localAudio" => {
            required_string(part, "path", location)?;
        }
        "skill" | "mention" => {
            required_string(part, "name", location)?;
            required_string(part, "path", location)?;
        }
        _ => {
            return Err(SourceError::new(
                "CODEX_TYPE_IS_AN_UNSUPPORTED_CODEX_USER_INPUT",
                "source_items",
                format!("{location}.type is an unsupported Codex user input"),
            ));
        }
    }
    Ok(())
}

pub(super) fn opaque_provider_evidence(
    item: &Value,
    provider_type: &str,
) -> Result<String, SourceError> {
    let bytes = serde_json::to_vec(item).map_err(|_| {
        SourceError::new(
            "CODEX_COULD_NOT_NORMALIZE_A_CODEX_PROVIDER_ITEM",
            "source_items",
            "could not normalize a Codex provider item",
        )
    })?;
    let mut evidence = json!({
        "provider_type": provider_type,
        "item_bytes": bytes.len(),
        "item_digest": format!("sha256:{:x}", Sha256::digest(&bytes)),
    });
    if provider_type == "contextCompaction" {
        evidence
            .as_object_mut()
            .expect("constructed object")
            .insert("compaction_boundary".to_owned(), Value::Bool(true));
    }
    serde_json::to_string(&evidence).map_err(|_| {
        SourceError::new(
            "CODEX_COULD_NOT_NORMALIZE_CODEX_PROVIDER_EVIDENCE",
            "source_items",
            "could not normalize Codex provider evidence",
        )
    })
}

fn validate_collaboration_item(item: &Value, location: &str) -> Result<(), SourceError> {
    required_object(item, "agentsStates", location)?;
    required_string_array(item, "receiverThreadIds", location)?;
    required_string(item, "senderThreadId", location)?;
    required_string(item, "status", location)?;
    required_string(item, "tool", location)?;
    Ok(())
}
