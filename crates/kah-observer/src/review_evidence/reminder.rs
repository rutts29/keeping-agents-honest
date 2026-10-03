use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntentQuote {
    pub id: String,
    pub quote: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reminder {
    pub instruction: String,
    pub quotes: Vec<IntentQuote>,
}
impl Reminder {
    pub fn render(&self, intent: &Value, record: &str) -> Result<String, String> {
        if self.instruction.trim().is_empty() || self.quotes.is_empty() {
            return Err("REVIEW_REMINDER_EMPTY stage=reminder".into());
        }
        let mut text = format!(
            "[kah verified observer; context, not new user steering] {}\n",
            self.instruction
        );
        for citation in &self.quotes {
            let original = if citation.id == "goal" {
                intent["goal"].as_str()
            } else {
                intent["items"]
                    .as_array()
                    .and_then(|items| items.iter().find(|i| i["id"] == citation.id))
                    .and_then(|i| i["text"].as_str())
            };
            if citation.quote.is_empty() || !original.is_some_and(|s| s.contains(&citation.quote)) {
                return Err("REVIEW_REMINDER_QUOTE stage=reminder".into());
            }
            text.push_str(&format!("{}: {}\n", citation.id, citation.quote));
        }
        text.push_str(&format!("Full intent record: {record}"));
        Ok(text)
    }
    pub fn schema() -> Value {
        json!({"type":["object","null"],"additionalProperties":false,"required":["instruction","quotes"],"properties":{"instruction":{"type":"string"},"quotes":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["id","quote"],"properties":{"id":{"type":"string"},"quote":{"type":"string"}}}}}})
    }
}
