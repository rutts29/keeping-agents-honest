//! Codex 0.162.1 (tested version) native items/turns APIs; pagination has no content cap.
use super::*;
use kah_codex::control::Control;
use serde_json::{Value, json};

pub(super) fn progress(
    client: &mut Control,
    session: &str,
    receipt: &str,
) -> Result<Option<(String, String)>> {
    let rows = kah_codex::source::full_history(client, session)?;
    let Some(row) = rows.iter().rev().find(|r| {
        r["item"]["type"] == "agentMessage"
            && r["item"]["text"]
                .as_str()
                .is_some_and(|t| t.contains(receipt))
    }) else {
        return Ok(None);
    };
    let reply = identity(&row["item"]["id"])?;
    let turn = identity(&row["turnId"])?;
    Ok(completed(client, session, &turn)?.then_some((reply, turn)))
}
fn completed(client: &mut Control, session: &str, turn: &str) -> Result<bool> {
    let mut cursor = Value::Null;
    let mut seen = BTreeSet::new();
    loop {
        let page = client.request(
            "thread/turns/list",
            json!({"threadId":session,"cursor":cursor,
            "limit":25,"sortDirection":"desc","itemsView":"notLoaded"}),
        )?;
        let turns = page["data"]
            .as_array()
            .ok_or("RUN_FOLLOWUP_NATIVE_TURNS stage=followup_progress")?;
        if let Some(found) = turns.iter().find(|t| t["id"] == turn) {
            return Ok(found["status"] == "completed");
        }
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            return Ok(false);
        }
        if !seen.insert(cursor.to_string()) {
            return Err("RUN_FOLLOWUP_NATIVE_CURSOR stage=followup_progress".into());
        }
    }
}
