use super::*;

pub(super) fn parse_thread_page(
    response: Value,
    canonical_root: &str,
) -> Result<(Vec<ThreadCandidate>, Option<String>), SourceError> {
    let data = required_array(&response, "data", "thread/list.result")?;
    let mut candidates = Vec::new();
    for (index, thread) in data.iter().enumerate() {
        let location = format!("thread/list.result.data[{index}]");
        let id = required_string(thread, "id", &location)?;
        let cwd = required_string(thread, "cwd", &location)?;
        if cwd != canonical_root {
            continue;
        }
        let recency = optional_i64(thread, "recencyAt", &location)?
            .or(optional_i64(thread, "updatedAt", &location)?)
            .ok_or_else(|| {
                SourceError::new(
                    "CODEX_RECENCYAT_MUST_BE_A_NUMBER_OR_NULL",
                    "source_listing",
                    format!("{location}.recencyAt must be a number or null"),
                )
            })?;
        candidates.push(ThreadCandidate { id, recency });
    }
    let next_cursor = match response.get("nextCursor") {
        Some(Value::Null) => None,
        Some(Value::String(cursor)) if !cursor.is_empty() && cursor.len() <= 4096 => {
            Some(cursor.clone())
        }
        Some(_) => {
            return Err(SourceError::new(
                "CODEX_THREAD_LIST_RESULT_NEXTCURSOR_MUST_BE_NULL_OR_A_NON",
                "source_listing",
                "thread/list.result.nextCursor must be null or a non-empty string",
            ));
        }
        None => {
            return Err(SourceError::new(
                "CODEX_THREAD_LIST_RESULT_NEXTCURSOR_IS_MISSING",
                "source_listing",
                "thread/list.result.nextCursor is missing",
            ));
        }
    };
    Ok((candidates, next_cursor))
}
