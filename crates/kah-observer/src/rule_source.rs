//! Replaceable source-linked rule list; transport and scheduling never atomize.
pub mod atomization;
use atomization::{AtomicRule, Item};
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
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Resolved {
    pub source_digest: String,
    pub observer_session: String,
    pub rules: Vec<Rule>,
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
            rules: self
                .rules
                .iter()
                .map(|r| Rule {
                    id: r.rule_id.clone(),
                    predicate: r.rule_sentence.clone(),
                    diff_checkable: r.diff_checkable,
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
