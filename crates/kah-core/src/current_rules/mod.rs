//! A source-linked derived rule set. History remains append-only authority.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
mod input;
mod validation;
pub use input::*;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Words {
    pub source: String,
    /// UTF-8 byte boundaries in the complete immutable source.
    pub start: usize,
    pub end: usize,
    pub quote: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Contributes,
    Merged,
    Superseded,
    ExpiredUnderCondition,
    Withdrawn,
    Context,
    RejectedJunk,
    UnresolvedConflict,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Clause {
    pub id: String,
    pub words: Words,
    pub disposition: Disposition,
    pub reason: String,
    pub rules: Vec<String>,
    pub witnesses: Vec<Words>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub predicate: String,
    pub scope: String,
    pub diff_checkable: bool,
    pub clauses: Vec<String>,
    /// Only exact permission/exception words linked to this rule.
    pub permissions: Vec<Words>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleSet {
    pub schema_version: u32,
    pub input_digest: String,
    pub previous_digest: Option<String>,
    pub rules: Vec<Rule>,
    pub clauses: Vec<Clause>,
}
/// Only validation can construct this type; callers cannot adopt raw model output.
#[derive(Clone, Debug)]
pub struct ValidatedSet(RuleSet);
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Question<'a> {
    pub rule_id: &'a str,
    pub predicate: &'a str,
    pub scope: &'a str,
    pub commit_sha: &'a str,
    pub input_digest: &'a str,
    pub permissions: Vec<&'a str>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SetDiff {
    pub added: Vec<String>,
    pub changed: Vec<String>,
    pub retired: Vec<String>,
    /// Clause provenance and dispositions are diffed as well as predicates.
    pub accounting_changed: bool,
}
impl ValidatedSet {
    pub fn as_set(&self) -> &RuleSet {
        &self.0
    }
    pub fn digest(&self) -> Result<String, String> {
        digest(&self.0)
    }
    pub fn has_conflicts(&self) -> bool {
        self.0
            .clauses
            .iter()
            .any(|c| c.disposition == Disposition::UnresolvedConflict)
    }
    /// Applicability remains explicit in each question; semantic scope is not guessed.
    pub fn questions<'a>(&'a self, commit_sha: &'a str) -> Result<Vec<Question<'a>>, String> {
        if self.has_conflicts() {
            return Err(error("CONFLICT", "direct observer review required"));
        }
        if commit_sha.trim().is_empty() {
            return Err(error("BINDING", "commit SHA required"));
        }
        Ok(self
            .0
            .rules
            .iter()
            .filter(|r| r.diff_checkable)
            .map(|r| Question {
                rule_id: &r.id,
                predicate: &r.predicate,
                scope: &r.scope,
                commit_sha,
                input_digest: &self.0.input_digest,
                permissions: r.permissions.iter().map(|p| p.quote.as_str()).collect(),
            })
            .collect())
    }
    pub fn diff(&self, previous: Option<&Self>) -> SetDiff {
        let old = previous.map(|p| p.0.rules.as_slice()).unwrap_or_default();
        SetDiff {
            added: self
                .0
                .rules
                .iter()
                .filter(|r| !old.iter().any(|p| p.id == r.id))
                .map(|r| r.id.clone())
                .collect(),
            changed: self
                .0
                .rules
                .iter()
                .filter(|r| old.iter().any(|p| p.id == r.id && p != *r))
                .map(|r| r.id.clone())
                .collect(),
            retired: old
                .iter()
                .filter(|p| !self.0.rules.iter().any(|r| p.id == r.id))
                .map(|p| p.id.clone())
                .collect(),
            accounting_changed: previous.is_none_or(|p| p.0.clauses != self.0.clauses),
        }
    }
}
pub fn digest(value: &(impl Serialize + ?Sized)) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(value).map_err(|e| format!("RULE_SET_JSON stage=current_rules: {e}"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}
pub(super) fn error(code: &str, detail: &str) -> String {
    format!("RULE_SET_{code} stage=current_rules: {detail}")
}
