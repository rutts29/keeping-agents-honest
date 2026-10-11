//! Replaceable source-linked rule list; transport and scheduling never atomize.
pub mod atomization;
mod current_set;
use atomization::{AtomicRule, Item};
pub use current_set::CurrentSet;
use serde::{Deserialize, Serialize};
pub trait Source {
    fn load(&self, items: &[Item]) -> Result<Resolved, String>;
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Link {
    pub source_id: String,
    pub quote: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    pub predicate: String,
    pub diff_checkable: bool,
    pub sources: Vec<Link>,
    #[serde(default)]
    pub scope: String,
    #[serde(default)]
    pub permissions: Vec<Link>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub source_digest: String,
    pub observer_session: String,
    pub rules: Vec<Rule>,
    #[serde(default)]
    pub source_words: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub history_digest: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct RuleList {
    pub input_digest: String,
    pub observer_session: String,
    pub rules: Vec<AtomicRule>,
}
impl Source for RuleList {
    fn load(&self, items: &[Item]) -> Result<Resolved, String> {
        let digest = crate::context_selection::digest(&serde_json::to_value(items).unwrap());
        if self.input_digest != digest || self.observer_session.is_empty() {
            return Err("COMMIT_RULE_SOURCE_CHANGED stage=rule_source".into());
        }
        atomization::verify(items, &self.rules)?;
        Ok(Resolved {
            source_digest: self.digest(),
            observer_session: self.observer_session.clone(),
            source_words: Default::default(),
            history_digest: None,
            rules: self
                .rules
                .iter()
                .map(|r| Rule {
                    id: r.rule_id.clone(),
                    predicate: r.rule_sentence.clone(),
                    diff_checkable: r.diff_checkable,
                    scope: String::new(),
                    permissions: vec![],
                    sources: vec![Link {
                        source_id: r.source_intent_id.clone(),
                        quote: r.source_quote.clone(),
                    }],
                })
                .collect(),
        })
    }
}
impl RuleList {
    pub fn digest(&self) -> String {
        crate::context_selection::digest(&serde_json::to_value(self).unwrap())
    }
}
