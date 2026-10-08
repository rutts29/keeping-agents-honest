//! C: only identical checked evidence may carry a confirmed light verdict.
//! Heavy gates always rejudge.
use kah_observer::decision_units::Dimension;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
mod capture;
pub use capture::record;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub authority: BTreeMap<String, String>,
    pub blobs: BTreeMap<String, String>,
    pub plans: BTreeMap<String, String>,
    pub components: BTreeMap<String, String>,
    pub tests: BTreeMap<String, String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub requirement: String,
    pub review: String,
    pub head: String,
    pub evidence: Evidence,
    pub confirmed: bool,
    pub always: bool,
    pub verdict: Value,
}
impl Entry {
    pub fn carry(&self, current: &Evidence, gate: bool) -> Option<Value> {
        (self.confirmed && !self.always && !gate && self.evidence == *current).then(|| {
            json!({"requirement":self.requirement,"carried_from_review":self.review,
                "carried_from_sha":self.head,"evidence":self.evidence,"verdict":self.verdict})
        })
    }
}
pub fn always(dimension: Dimension, unconditional: bool, claim: bool, user_wait: bool) -> bool {
    matches!(dimension, Dimension::Drift | Dimension::Steering)
        || unconditional
        || claim
        || user_wait
}
#[cfg(test)]
#[path = "../../tests/observer_unit/verdict_cache.rs"]
mod tests;
