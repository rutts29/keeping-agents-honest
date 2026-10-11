//! Production commit screen: policy is data; native reviews own confirmation.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub mod checkpoint;
pub mod recalibration;
pub mod transport;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DoubtLine {
    pub line: f64,
    pub provenance: Vec<String>,
    pub adjustment: String,
}
impl DoubtLine {
    pub fn validate(&self) -> Result<(), String> {
        if !self.line.is_finite()
            || !(0.0..=1.0).contains(&self.line)
            || self.provenance.is_empty()
            || self.adjustment.trim().is_empty()
        {
            return Err("COMMIT_POLICY_INVALID stage=commit_review".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Noul {
    pub request: String,
    pub question: String,
    pub noul: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub worker: String,
    pub head: String,
    pub policy: DoubtLine,
    pub answers: Vec<Noul>,
    pub maximum: Option<Noul>,
    pub woke: bool,
    pub gaps: Vec<Value>,
    pub native: Option<NativeVerdict>,
    pub checkpoints: BTreeMap<String, Vec<crate::commit_check::Row>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeVerdict {
    pub review: String,
    pub observer_session: String,
    pub authority_digest: String,
    pub files: BTreeMap<String, String>,
    pub rows: Vec<crate::commit_check::Row>,
}
pub fn classify(
    worker: &str,
    head: &str,
    answers: &[Value],
    policy: &DoubtLine,
) -> Result<Review, String> {
    policy.validate()?;
    let answers: Vec<Noul> = serde_json::from_value(Value::Array(answers.to_vec()))
        .map_err(|_| "COMMIT_ANSWERS_INVALID stage=commit_review")?;
    if answers
        .iter()
        .any(|a| !a.noul.is_finite() || !(0.0..=1.0).contains(&a.noul))
    {
        return Err("COMMIT_PROBABILITY_INVALID stage=commit_review".into());
    }
    let maximum = answers
        .iter()
        .max_by(|a, b| a.noul.total_cmp(&b.noul))
        .cloned();
    let woke = maximum.as_ref().is_some_and(|m| m.noul >= policy.line);
    Ok(Review {
        worker: worker.into(),
        head: head.into(),
        policy: policy.clone(),
        answers,
        maximum,
        woke,
        gaps: vec![],
        native: None,
        checkpoints: BTreeMap::new(),
    })
}
impl Review {
    pub fn requires_native(&self) -> bool {
        self.woke || !self.gaps.is_empty()
    }
    pub fn confirm(
        &mut self,
        input: &crate::commit_check::Input,
        verdict: NativeVerdict,
    ) -> Result<(), String> {
        if input.revision != self.head
            || verdict.observer_session.is_empty()
            || verdict.authority_digest
                != crate::context_selection::digest(&serde_json::to_value(&input.intent).unwrap())
        {
            return Err("COMMIT_NATIVE_BINDING stage=commit_review".into());
        }
        let expected = input
            .rules
            .keys()
            .collect::<std::collections::BTreeSet<_>>();
        let mut seen = std::collections::BTreeSet::new();
        for row in &verdict.rows {
            if !expected.contains(&row.requirement) || !seen.insert(&row.requirement) {
                return Err("COMMIT_NATIVE_SCOPE stage=commit_review".into());
            }
            if row.status == crate::commit_check::Status::Violated && row.quotes.is_empty() {
                return Err("COMMIT_NATIVE_EVIDENCE stage=commit_review".into());
            }
            for quote in &row.quotes {
                crate::commit_check::verify_quote(input, quote)?;
            }
        }
        if seen != expected {
            return Err("COMMIT_NATIVE_COVERAGE stage=commit_review".into());
        }
        if verdict.files
            != input
                .files
                .iter()
                .map(|(p, f)| (p.clone(), f.digest.clone()))
                .collect()
        {
            return Err("COMMIT_NATIVE_FILES stage=commit_review".into());
        }
        self.native = Some(verdict);
        Ok(())
    }
}
