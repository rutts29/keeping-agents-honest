//! A model proposes evidence, never probabilities or decisions. The host checks
//! every quote against its immutable full-context snapshot before invoking Jev.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub id: String,
    pub kind: String,
    pub uri: String,
    pub text: String,
    pub digest: String,
}
impl Document {
    pub fn new(id: &str, kind: &str, uri: &str, text: &str) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            uri: uri.into(),
            text: text.into(),
            digest: format!("sha256:{:x}", Sha256::digest(text.as_bytes())),
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
pub struct Finding {
    pub topic: Topic,
    pub claim: String,
    pub citations: Vec<Citation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub request_id: String,
    pub findings: Vec<Finding>,
}
impl Report {
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
            Ok(json!({"topic":finding.topic,"model_claim_not_a_decision":finding.claim,"citations":citations}))
        }).collect()
    }
    pub fn schema() -> Value {
        json!({"type":"object","additionalProperties":false,"required":["request_id","findings"],"properties":{
        "request_id":{"type":"string"},"findings":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["topic","claim","citations"],"properties":{
            "topic":{"type":"string","enum":["intent","integration","deferral","drift","steering","claim"]},"claim":{"type":"string"},"citations":{"type":"array","minItems":1,"items":{"type":"object","additionalProperties":false,"required":["source","digest","quote"],"properties":{"source":{"type":"string"},"digest":{"type":"string"},"quote":{"type":"string"},"occurrence":{"type":"integer","minimum":0}}}}
        }}}}})
    }
}
fn verify_quote(citation: &Citation, documents: &[Document]) -> Result<Value, String> {
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
    let line = document.text[..start]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1;
    // Preserve the source context, including negations outside the exact quote.
    Ok(
        json!({"source":document.id,"kind":document.kind,"uri":document.uri,"digest":digest,"quote":citation.quote,"start_byte":start,"line":line,"context":document.text,"context_is_excerpt":false}),
    )
}
