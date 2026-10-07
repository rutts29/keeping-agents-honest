//! Host receipts are distinct from native worker output; comparisons need no vote.
use super::Requirement;
use crate::{
    review_evidence::{Document, verify_quote},
    verification_contract::ReceiptFact,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub worker: String,
    pub request: String,
    pub head: String,
    pub receipt_citation: usize,
    pub property: ReceiptFact,
}
impl Fact {
    pub fn label(&self) -> String {
        match &self.property {
            ReceiptFact::HeadSha => format!("Controller delivery HEAD {}", self.head),
            ReceiptFact::TestExit {
                test_index,
                expected,
            } => format!("Controller check {test_index} exit status {expected}"),
            ReceiptFact::TestOutputLine {
                test_index,
                expected,
            } => format!(
                "Controller check {test_index} output line {}",
                json!(expected)
            ),
            ReceiptFact::TagHead { tag } => {
                format!("Controller tag {} targets {}", json!(tag), self.head)
            }
        }
    }
    pub fn verify(&self, r: &Requirement, documents: &[Document]) -> Result<(), String> {
        if r.name != self.label()
            || !r.components.is_empty()
            || !r.checks.is_empty()
            || r.blocker.is_some()
            || r.deferral_id.is_some()
        {
            return Err("REVIEW_CONTROLLER_FACT_SCOPE stage=receipt: only the canonical literal property, not behavior or semantics".into());
        }
        let citation = r
            .citations
            .get(self.receipt_citation)
            .ok_or("REVIEW_CONTROLLER_FACT_CITATION stage=receipt")?;
        verify_quote(citation, documents)?;
        let doc = documents
            .iter()
            .find(|d| d.id == citation.source)
            .ok_or("REVIEW_CONTROLLER_FACT_SOURCE stage=receipt")?;
        if doc.id != "controller-receipts" || doc.kind != "controller_verification" {
            return Err("REVIEW_CONTROLLER_FACT_SOURCE stage=receipt: worker output cannot create a host receipt".into());
        }
        let value: Value = serde_json::from_str(&doc.text)
            .map_err(|_| "REVIEW_CONTROLLER_FACT_JSON stage=receipt")?;
        if value["worker"] != self.worker
            || value["request"] != self.request
            || value["head"] != self.head
            || value["fresh_checkout"] != true
            || !oid(&self.head)
        {
            return Err("REVIEW_CONTROLLER_FACT_BINDING stage=receipt".into());
        }
        if !self.matches(&value) {
            return Err("REVIEW_CONTROLLER_FACT_CONTRADICTS stage=receipt".into());
        }
        Ok(())
    }
    fn matches(&self, value: &Value) -> bool {
        match &self.property {
            ReceiptFact::HeadSha => true,
            ReceiptFact::TestExit {
                test_index,
                expected,
            } => test(value, *test_index, &self.head).is_some_and(|t| t["exit_code"] == *expected),
            ReceiptFact::TestOutputLine {
                test_index,
                expected,
            } => {
                !expected.contains(['\n', '\r'])
                    && !expected.trim().is_empty()
                    && test(value, *test_index, &self.head).is_some_and(|t| {
                        t["output"]
                            .as_str()
                            .is_some_and(|s| s.lines().any(|line| line == expected))
                    })
            }
            ReceiptFact::TagHead { tag } => value["tags"].as_array().is_some_and(|tags| {
                tags.iter()
                    .any(|t| t["name"] == *tag && t["head"] == self.head)
            }),
        }
    }
}
fn test<'a>(value: &'a Value, index: usize, head: &str) -> Option<&'a Value> {
    value["tests"].get(index).filter(|t| t["commit"] == head)
}
fn oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(super) fn schema() -> Value {
    super::schema::object(
        &["worker", "request", "head", "receipt_citation", "property"],
        json!({
            "worker":{"type":"string"},"request":{"type":"string"},"head":{"type":"string"},
            "receipt_citation":{"type":"integer","minimum":0},
            "property":crate::verification_contract::receipt_schema()["items"].clone()
        }),
    )
}
