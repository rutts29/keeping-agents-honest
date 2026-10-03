//! Event-count debounce for completed native tools; no clock or polling trigger.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
const BATCH: usize = 4;
const RECENT: usize = 32;

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Progress {
    turn: String,
    seen: VecDeque<String>,
    pending: Vec<Value>,
}
impl Progress {
    pub fn push(
        &mut self,
        turn: &str,
        id: &str,
        action: Value,
    ) -> Result<Option<Vec<Value>>, String> {
        self.validate()?;
        if !valid_id(turn) || !valid_id(id) {
            return Err("OBSERVER_TOOL_EVENT stage=tool_watch: invalid native identifier".into());
        }
        if self.turn != turn {
            self.turn = turn.into();
            self.seen.clear();
        }
        if self.seen.iter().any(|seen| seen == id) {
            return Ok(None);
        }
        self.seen.push_back(id.into());
        if self.seen.len() > RECENT {
            self.seen.pop_front();
        }
        self.pending.push(action);
        Ok((self.pending.len() == BATCH).then(|| std::mem::take(&mut self.pending)))
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.seen.len() > RECENT
            || self.pending.len() >= BATCH
            || self.turn.len() > 512
            || self.seen.iter().any(|id| !valid_id(id))
        {
            return Err("OBSERVER_TOOL_STATE stage=tool_watch: invalid bounded progress".into());
        }
        Ok(())
    }
}
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 512 && !id.chars().any(char::is_control)
}
