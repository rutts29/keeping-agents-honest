//! One private controller round; shared native/delegate text cannot create it.
use super::*;
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub worker: String,
    pub review: String,
    pub head: String,
    pub intent_revision: u64,
}
impl Binding {
    pub fn from_request(request: &Request) -> Result<Self> {
        let binding = Self {
            worker: request.worker.clone(),
            review: request.id.clone(),
            head: request
                .head
                .clone()
                .ok_or("REVIEW_FOLLOWUP_HEAD stage=followup")?,
            intent_revision: request.intent["revision"]
                .as_u64()
                .ok_or("REVIEW_FOLLOWUP_REVISION stage=followup")?,
        };
        if !oid(&binding.head)
            || binding.worker.is_empty()
            || !binding.review.starts_with("review-")
        {
            return Err("REVIEW_FOLLOWUP_BINDING stage=followup".into());
        }
        Ok(binding)
    }
    pub fn id(&self) -> Result<String> {
        Ok(format!(
            "followup-{:x}",
            Sha256::digest(serde_json::to_vec(self)?)
        ))
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Evidence,
    Builder,
    Checks,
    Complete,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvidencePointer {
    pub receipt: String,
    pub record: String,
    pub digest: String,
}
impl EvidencePointer {
    fn valid(&self) -> bool {
        !self.receipt.trim().is_empty()
            && !self.record.trim().is_empty()
            && self
                .digest
                .strip_prefix("sha256:")
                .unwrap_or(&self.digest)
                .len()
                == 64
            && self
                .digest
                .strip_prefix("sha256:")
                .unwrap_or(&self.digest)
                .bytes()
                .all(|b| b.is_ascii_hexdigit())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum BuilderOutcome {
    Evidence { response: EvidencePointer },
    Unavailable { reason: String },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuilderReceipt {
    pub request: EvidencePointer,
    pub response: BuilderOutcome,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    pub what: String,
    pub receipt: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnverifiedRisk {
    pub requirement_id: String,
    pub unverified: String,
    pub tried: Vec<Attempt>,
    pub acceptance_risk: String,
    pub evidence: Vec<EvidencePointer>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckReceipt {
    pub receipt: EvidencePointer,
    pub proof: DeliveryProof,
    pub risks: Vec<UnverifiedRisk>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "stage", content = "receipt", rename_all = "snake_case")]
pub enum Step {
    Evidence(Vec<EvidencePointer>),
    Builder(BuilderReceipt),
    Checks(CheckReceipt),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationRound {
    pub id: String,
    pub binding: Binding,
    pub stage: Stage,
    pub claimed: bool,
    pub evidence: Option<Vec<EvidencePointer>>,
    pub builder: Option<BuilderReceipt>,
    pub checks: Option<CheckReceipt>,
}
impl VerificationRound {
    pub fn builder_receipt(&self) -> String {
        format!("{}-builder", self.id)
    }
    pub fn checks_receipt(&self) -> String {
        format!("{}-checks", self.id)
    }
}
/// Constructible only by reading and validating the private controller record.
pub struct CompletedFollowup {
    round: VerificationRound,
    admission: Binding,
}
impl CompletedFollowup {
    pub fn round(&self) -> &VerificationRound {
        &self.round
    }
    pub fn risks(&self) -> &[UnverifiedRisk] {
        &self.round.checks.as_ref().unwrap().risks
    }
    pub(super) fn matches(&self, request: &Request) -> bool {
        Binding::from_request(request).is_ok_and(|binding| binding == self.admission)
    }
}

fn directory(root: &Path) -> PathBuf {
    root.join("gate-followups")
}
pub fn read(root: &Path, binding: &Binding) -> Result<Option<VerificationRound>> {
    let path = directory(root).join(format!("{}.json", binding.id()?));
    if !path.exists() {
        return Ok(None);
    }
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|error| format!("REVIEW_FOLLOWUP_IO stage=followup: {error}"))?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o077 != 0 {
        return Err("REVIEW_FOLLOWUP_PRIVATE stage=followup".into());
    }
    let round: VerificationRound = super::super::source_store::read(&path)?;
    if round.binding != *binding || round.id != binding.id()? {
        return Err(
            "REVIEW_FOLLOWUP_BINDING stage=followup: private round identity changed".into(),
        );
    }
    validate(&round)?;
    Ok(Some(round))
}
pub fn start(root: &Path, request: &Request) -> Result<VerificationRound> {
    let binding = origin(root, request)?;
    let _lock = super::super::state::Store::delivery_at(&directory(root), true)?;
    if let Some(round) = read(root, &binding)? {
        return Ok(round);
    }
    let round = VerificationRound {
        id: binding.id()?,
        binding,
        stage: Stage::Evidence,
        claimed: false,
        evidence: None,
        builder: None,
        checks: None,
    };
    save(root, &round)?;
    Ok(round)
}
fn save(root: &Path, round: &VerificationRound) -> Result<()> {
    super::super::source_store::save(
        &directory(root),
        &format!("{}.json", round.id),
        &serde_json::to_value(round)?,
    )
}
/// Reserve an external operation once before delivery/check execution. An
/// uncertain result is reconciled from its stable receipt, never repeated.
pub fn claim(root: &Path, binding: &Binding, stage: Stage) -> Result<bool> {
    let _lock = super::super::state::Store::delivery_at(&directory(root), true)?;
    let mut round = read(root, binding)?.ok_or("REVIEW_FOLLOWUP_MISSING stage=followup")?;
    if round.stage != stage || stage == Stage::Complete {
        return Ok(false);
    }
    if round.claimed {
        return Ok(false);
    }
    round.claimed = true;
    save(root, &round)?;
    Ok(true)
}
pub fn advance(root: &Path, binding: &Binding, step: Step) -> Result<VerificationRound> {
    let _lock = super::super::state::Store::delivery_at(&directory(root), true)?;
    let mut round = read(root, binding)?.ok_or("REVIEW_FOLLOWUP_MISSING stage=followup")?;
    let (stage, old) = match &step {
        Step::Evidence(_) => (Stage::Evidence, serde_json::to_value(&round.evidence)?),
        Step::Builder(_) => (Stage::Builder, serde_json::to_value(&round.builder)?),
        Step::Checks(_) => (Stage::Checks, serde_json::to_value(&round.checks)?),
    };
    let incoming = match &step {
        Step::Evidence(v) => serde_json::to_value(v)?,
        Step::Builder(v) => serde_json::to_value(v)?,
        Step::Checks(v) => serde_json::to_value(v)?,
    };
    if !old.is_null() {
        return if old == incoming {
            Ok(round)
        } else {
            Err("REVIEW_FOLLOWUP_RECEIPT_CHANGED stage=followup".into())
        };
    }
    if round.stage != stage {
        return Err("REVIEW_FOLLOWUP_ORDER stage=followup".into());
    }
    match step {
        Step::Evidence(v) => {
            round.evidence = Some(v);
            round.stage = Stage::Builder;
        }
        Step::Builder(v) => {
            round.builder = Some(v);
            round.stage = Stage::Checks;
        }
        Step::Checks(v) => {
            round.checks = Some(v);
            round.stage = Stage::Complete;
        }
    }
    round.claimed = false;
    validate(&round)?;
    save(root, &round)?;
    Ok(round)
}
fn validate(round: &VerificationRound) -> Result<()> {
    let valid_evidence = round
        .evidence
        .as_ref()
        .is_some_and(|v| !v.is_empty() && v.iter().all(EvidencePointer::valid));
    let valid_builder = round.builder.as_ref().is_some_and(|v| {
        v.request.valid()
            && v.request.receipt == round.builder_receipt()
            && match &v.response {
                BuilderOutcome::Evidence { response } => response.valid(),
                BuilderOutcome::Unavailable { reason } => !reason.trim().is_empty(),
            }
    });
    let valid = match round.stage {
        Stage::Evidence => {
            round.evidence.is_none() && round.builder.is_none() && round.checks.is_none()
        }
        Stage::Builder => valid_evidence && round.builder.is_none() && round.checks.is_none(),
        Stage::Checks => valid_evidence && valid_builder && round.checks.is_none(),
        Stage::Complete => {
            valid_evidence
                && valid_builder
                && round.checks.as_ref().is_some_and(|v| {
                    v.receipt.valid()
                        && v.receipt.receipt == round.checks_receipt()
                        && v.proof.passes(&round.binding.head)
                })
                && valid_risks(round)
        }
    };
    if !valid {
        return Err(
            "REVIEW_FOLLOWUP_RECEIPTS stage=followup: incomplete or invalid stage evidence".into(),
        );
    }
    Ok(())
}
fn valid_risks(round: &VerificationRound) -> bool {
    let Some(checks) = &round.checks else {
        return false;
    };
    let mut pointers = round.evidence.as_ref().unwrap().iter().collect::<Vec<_>>();
    let builder = round.builder.as_ref().unwrap();
    pointers.push(&builder.request);
    if let BuilderOutcome::Evidence { response } = &builder.response {
        pointers.push(response)
    }
    pointers.push(&checks.receipt);
    checks.risks.iter().all(|r| {
        !r.requirement_id.trim().is_empty()
            && !r.unverified.trim().is_empty()
            && !r.acceptance_risk.trim().is_empty()
            && !r.tried.is_empty()
            && !r.evidence.is_empty()
            && r.tried.iter().all(|a| {
                !a.what.trim().is_empty() && pointers.iter().any(|p| p.receipt == a.receipt)
            })
            && r.evidence
                .iter()
                .all(|p| p.valid() && pointers.contains(&p))
    })
}
pub fn completed(root: &Path, request: &Request) -> Result<Option<CompletedFollowup>> {
    let binding = origin(root, request)?;
    let admission = Binding::from_request(request)?;
    Ok(read(root, &binding)?
        .filter(|r| r.stage == Stage::Complete)
        .map(|round| CompletedFollowup { round, admission }))
}
/// Follow only immutable controller archives, with the same checkpoint scope.
pub fn origin(root: &Path, request: &Request) -> Result<Binding> {
    let expected = Binding::from_request(request)?;
    let mut current = archived(root, &request.id)?;
    if serde_json::to_value(&current)? != serde_json::to_value(request)? {
        return Err("REVIEW_FOLLOWUP_LINEAGE stage=followup: admitting request differs from controller archive".into());
    }
    let mut visited = std::collections::BTreeSet::new();
    loop {
        let binding = Binding::from_request(&current)?;
        if !visited.insert(binding.review.clone())
            || binding.worker != expected.worker
            || binding.head != expected.head
            || binding.intent_revision != expected.intent_revision
        {
            return Err("REVIEW_FOLLOWUP_LINEAGE stage=followup: checkpoint scope changed or lineage cycles".into());
        }
        let Some(parent) = current.trigger["reconsideration"]["request"].as_str() else {
            return Ok(binding);
        };
        current = archived(root, parent)?;
    }
}
fn archived(root: &Path, id: &str) -> Result<Request> {
    if !id.starts_with("review-") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(
            "REVIEW_FOLLOWUP_LINEAGE stage=followup: invalid controller review pointer".into(),
        );
    }
    let request: Request =
        super::super::source_store::read(&root.join("requests").join(format!("{id}.json")))?;
    if request.id != id || request.purpose != Purpose::Checkpoint {
        return Err(
            "REVIEW_FOLLOWUP_LINEAGE stage=followup: archive is not the named checkpoint".into(),
        );
    }
    Ok(request)
}
