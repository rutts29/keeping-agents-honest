//! Usage-calibrated planning, never a content limit or tokenizer substitute.
use crate::judge::Questions;
use serde::{Deserialize, Serialize};
use serde_json::Value;
pub const STATE_AND_LONGEST_TOKENS: f64 = 32_000.0;
pub const REQUEST_TOKENS: f64 = 64_000.0;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Calibration {
    pub input_utf8_bytes: u64,
    pub input_tokens: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub estimated_state_and_longest_tokens: f64,
    pub estimated_request_tokens: f64,
    pub prepare: bool,
    pub planning_only: bool,
}
impl Calibration {
    pub fn plan(&self, state: &Value, questions: &Questions) -> Option<Plan> {
        if self.input_utf8_bytes == 0 || self.input_tokens == 0 {
            return None;
        }
        let ratio = self.input_tokens as f64 / self.input_utf8_bytes as f64;
        let state = state.to_string().len() as f64;
        let lengths = questions
            .values()
            .map(|q| serde_json::to_vec(q).expect("question serialization").len() as f64)
            .collect::<Vec<_>>();
        let longest = lengths.iter().copied().fold(0.0, f64::max);
        let total = lengths.iter().sum::<f64>();
        let state_and_longest = (state + longest) * ratio;
        let request = (state + total) * ratio;
        Some(Plan {
            estimated_state_and_longest_tokens: state_and_longest,
            estimated_request_tokens: request,
            prepare: state_and_longest > STATE_AND_LONGEST_TOKENS || request > REQUEST_TOKENS,
            planning_only: true,
        })
    }
}
