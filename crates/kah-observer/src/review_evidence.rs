//! The thinker supplies evidence and typed semantic decisions, never model
//! probabilities. Host verifies original sources before Jev validation.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
mod native_record;
mod reminder;
mod verified_sources;
pub use native_record::NativeRecord;
pub use reminder::{IntentQuote, Reminder};
pub use verified_sources::VerifiedSources;

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
pub struct VerificationRisk {
    pub requirement_id: String,
    pub unverified: String,
    pub acceptance_risk: String,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_risks: Option<Vec<VerificationRisk>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_contracts: Option<Vec<crate::verification_contract::Item>>,
    /// Declarative project metadata; the host validates its native impact map.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_map: Option<Value>,
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
        let sources = VerifiedSources::new(documents);
        self.findings.iter().map(|finding| {
            if finding.claim.trim().is_empty() || finding.citations.is_empty() {
                return Err("REVIEW_FINDING_EMPTY stage=evidence".into());
            }
            let citations = finding.citations.iter().map(|citation| sources.quote(citation)).collect::<Result<Vec<_>,_>>()?;
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
        value["properties"]["verification_contracts"] = crate::verification_contract::schema();
        value["properties"]["verification_risks"] = json!({"type":"array","items":{
            "type":"object","additionalProperties":false,"required":["requirement_id","unverified","acceptance_risk"],
            "properties":{"requirement_id":{"type":"string"},"unverified":{"type":"string"},"acceptance_risk":{"type":"string"}}
        }});
        value
    }
}
pub(crate) fn verify_quote(citation: &Citation, documents: &[Document]) -> Result<Value, String> {
    VerifiedSources::new(documents).quote(citation)
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
