//! The thinker supplies evidence and typed semantic decisions, never model
//! probabilities. Host verifies original sources before Jev validation.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod native_record;
mod reminder;
pub use native_record::NativeRecord;
pub use reminder::{IntentQuote, Reminder};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub uri: String,
    pub text: String,
    pub digest: String,
    #[serde(
        default,
        with = "native_record::encoded",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub native_records: Vec<NativeRecord>,
}
impl Document {
    pub fn new(id: &str, kind: &str, uri: &str, text: &str) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            uri: uri.into(),
            text: text.into(),
            digest: format!("sha256:{:x}", Sha256::digest(text.as_bytes())),
            native_records: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Citation {
    pub source: String,
    pub digest: String,
    pub quote: String,
    #[serde(default)]
    pub occurrence: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Intent,
    Integration,
    Deferral,
    Drift,
    Steering,
    Claim,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deferral {
    #[serde(default)]
    pub id: Option<String>,
    pub phase: String,
    pub requirement_citation: usize,
    pub dependency_citation: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointScope {
    pub phase: String,
    pub components: Vec<String>,
    pub authorization_citation: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub topic: Topic,
    pub claim: String,
    pub citations: Vec<Citation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferral: Option<Deferral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_scope: Option<CheckpointScope>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub request_id: String,
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminder: Option<Reminder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_units: Option<crate::decision_units::ReviewUnits>,
}
impl Report {
    pub fn verify_units(
        &self,
        documents: &[Document],
        active_items: &[String],
    ) -> Result<Option<Value>, String> {
        self.decision_units
            .as_ref()
            .map(|u| u.verify(documents, active_items))
            .transpose()
    }
    pub fn reminder_text(&self, intent: &Value, record: &str) -> Result<String, String> {
        self.reminder
            .as_ref()
            .ok_or("REVIEW_REMINDER_MISSING stage=reminder")?
            .render(intent, record)
    }
    pub fn verify(&self, request: &str, documents: &[Document]) -> Result<Vec<Value>, String> {
        if self.request_id != request {
            return Err("REVIEW_REQUEST_MISMATCH stage=evidence".into());
        }
        if self.findings.is_empty() {
            return Err("REVIEW_EVIDENCE_EMPTY stage=evidence".into());
        }
        self.findings.iter().map(|finding| {
            if finding.claim.trim().is_empty() || finding.citations.is_empty() {
                return Err("REVIEW_FINDING_EMPTY stage=evidence".into());
            }
            let citations = finding.citations.iter().map(|citation| verify_quote(citation, documents)).collect::<Result<Vec<_>,_>>()?;
            let mut value=json!({"topic":finding.topic,"model_claim_not_a_decision":finding.claim,"citations":citations});
            if let Some(scope) = &finding.checkpoint_scope {
                if !matches!(finding.topic, Topic::Integration) || scope.phase.trim().is_empty()
                    || scope.components.is_empty() || scope.components.iter().any(|c| c.trim().is_empty())
                    || citations.get(scope.authorization_citation).is_none() {
                    return Err("REVIEW_CHECKPOINT_SCOPE stage=evidence: name the current phase and components with a verified authorization citation".into());
                }
                value["checkpoint_scope"] = serde_json::to_value(scope).map_err(|_| "REVIEW_CHECKPOINT_SCOPE_JSON stage=evidence")?;
            }
            if let Some(deferral)=&finding.deferral {
                let dependency=citations.get(deferral.dependency_citation).ok_or("REVIEW_DEFERRAL_CITATION stage=evidence")?;
                if citations.get(deferral.requirement_citation).is_none() || deferral.phase.trim().is_empty() || !dependency["quote"].as_str().is_some_and(|q|q.contains(&deferral.phase)) {
                    return Err("REVIEW_DEFERRAL_PHASE stage=evidence: named phase needs an exact dependency citation".into());
                }
                value["deferral"]=serde_json::to_value(deferral).map_err(|_|"REVIEW_DEFERRAL_JSON stage=evidence")?;
            }
            Ok(value)
        }).collect()
    }
    pub fn schema() -> Value {
        let mut value = json!({"type":"object","additionalProperties":false,"required":["request_id","findings"],"properties":{"reminder":Reminder::schema(),
        "request_id":{"type":"string"},"findings":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["topic","claim","citations"],"properties":{
            "topic":{"type":"string","enum":["intent","integration","deferral","drift","steering","claim"]},"claim":{"type":"string"},"checkpoint_scope":{"type":"object","additionalProperties":false,"required":["phase","components","authorization_citation"],"properties":{"phase":{"type":"string"},"components":{"type":"array","minItems":1,"items":{"type":"string"}},"authorization_citation":{"type":"integer","minimum":0}}},"deferral":{"type":"object","additionalProperties":false,"required":["phase","requirement_citation","dependency_citation"],"properties":{"id":{"type":["string","null"]},"phase":{"type":"string"},"requirement_citation":{"type":"integer","minimum":0},"dependency_citation":{"type":"integer","minimum":0}}},"citations":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["source","digest","quote"],"properties":{"source":{"type":"string"},"digest":{"type":"string"},"quote":{"type":"string"},"occurrence":{"type":"integer","minimum":0}}}}
        }}}}});
        let mut units = crate::decision_units::schema::schema();
        value["$defs"] = units.as_object_mut().unwrap().remove("$defs").unwrap();
        value["properties"]["decision_units"] = units;
        value
    }
}
pub(crate) fn verify_quote(citation: &Citation, documents: &[Document]) -> Result<Value, String> {
    let document = documents
        .iter()
        .find(|d| d.id == citation.source)
        .ok_or("REVIEW_SOURCE_UNKNOWN stage=evidence")?;
    let digest = format!("sha256:{:x}", Sha256::digest(document.text.as_bytes()));
    if citation.digest != document.digest || digest != document.digest {
        return Err("REVIEW_SOURCE_CHANGED stage=evidence".into());
    }
    if citation.quote.is_empty() {
        return Err("REVIEW_QUOTE_EMPTY stage=evidence".into());
    }
    let (start, _) = document
        .text
        .match_indices(&citation.quote)
        .nth(citation.occurrence)
        .ok_or("REVIEW_QUOTE_MISSING stage=evidence")?;
    let end = start + citation.quote.len();
    if joins_word(
        document.text[..start].chars().next_back(),
        citation.quote.chars().next(),
    ) || joins_word(
        citation.quote.chars().next_back(),
        document.text[end..].chars().next(),
    ) {
        return Err("REVIEW_QUOTE_UNIT stage=evidence: the brain must select a complete meaningful unit, never part of a word".into());
    }
    let line = document.text[..start]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    // The brain chooses a complete meaningful span, including any nearby
    // context it needs. Keep the full source in the immutable snapshot/CAS;
    // repeating it here would undo evidence extraction and swamp Jev.
    let evidence = json!({"source":document.id,"kind":document.kind,"uri":document.uri,"digest":digest,"quote":citation.quote,"start_byte":start,"end_byte":end,"line":line});
    Ok(with_native_records(evidence, document, start, end))
}
fn with_native_records(
    mut evidence: Value,
    document: &Document,
    start: usize,
    end: usize,
) -> Value {
    let records = document
        .native_records
        .iter()
        .filter(|r| r.start_byte < end && r.end_byte > start)
        .map(|r| r.context.clone())
        .collect::<Vec<_>>();
    if !records.is_empty() {
        evidence["native_records"] = json!(records);
    }
    evidence
}
fn joins_word(left: Option<char>, right: Option<char>) -> bool {
    left.is_some_and(|c| c.is_alphanumeric() || c == '_')
        && right.is_some_and(|c| c.is_alphanumeric() || c == '_')
}
