//! Verification remains tied to exact completion inputs while a review runs.
use crate::Result;
use kah_run::verification::Receipt;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key {
    pub instance: String,
    pub commit: String,
    pub user_sequence: u64,
    pub action_digest: String,
    pub command: String,
    pub online: bool,
}
impl Key {
    /// Evidence-only replies change action freshness, not the delivered code.
    /// SHA, user sequence and test policy bind it. Controller lease/instance
    /// freshness is enforced by the runner; a restart is not different code.
    pub fn same_delivery(&self, other: &Self) -> bool {
        self.commit == other.commit
            && self.user_sequence == other.user_sequence
            && self.command == other.command
            && self.online == other.online
    }
}
#[derive(Serialize, Deserialize)]
pub struct Verified {
    pub key: Key,
    pub receipt: Receipt,
    pub changed: bool,
}
impl Verified {
    pub fn id(&self) -> Result<String> {
        use sha2::{Digest, Sha256};
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&self.key)?)
        ))
    }
}
pub fn verified(
    directory: &Path,
    key: &Key,
    run: impl FnOnce() -> Result<(Receipt, bool)>,
) -> Result<Verified> {
    let path = directory.join("completion-verification.json");
    if let Some(saved) = crate::observer_review::state::read::<Verified>(&path)?
        && saved.key.same_delivery(key)
    {
        validate(key, &saved.receipt)?;
        return Ok(saved);
    }
    let (receipt, changed) = run()?;
    validate(key, &receipt)?;
    let result = Verified {
        key: key.clone(),
        receipt,
        changed,
    };
    crate::unattended_report::save(
        directory,
        "completion-verification.json",
        &serde_json::to_value(&result)?,
    )?;
    Ok(result)
}
fn validate(key: &Key, receipt: &Receipt) -> Result<()> {
    if receipt.commit != key.commit || receipt.command != key.command {
        return Err("RUN_VERIFICATION_IDENTITY stage=completion: receipt belongs to different completion inputs".into());
    }
    Ok(())
}
