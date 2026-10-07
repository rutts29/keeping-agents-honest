//! Reuse fsck-imported private objects; select the round SHA in a fresh checkout.
use super::*;
use kah_sandbox::LaunchOptions;

pub(super) fn execute(
    root: &Path,
    state: &super::super::state::State,
    round: &followup::VerificationRound,
    original: &DeliveryProof,
) -> Result<DeliveryProof> {
    if !original.passes(&round.binding.head) {
        return Err("REVIEW_FOLLOWUP_DELIVERY stage=verification".into());
    }
    let home =
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=verification")?);
    let directory = root
        .join("gate-followups")
        .join(format!("{}-checks", round.id));
    kah_core::run_store::ensure_private_directory(&directory)?;
    let candidate = trusted_candidate(state)?;
    let options = options(&state.worker.session)?;
    let mut proof = DeliveryProof {
        head: round.binding.head.clone(),
        fresh_checkout: true,
        tests: vec![],
        artifacts: original.artifacts.clone(),
    };
    for (index, test) in original.tests.iter().enumerate() {
        let (receipt, changed) = crate::exact_verification::execute(
            &home,
            &candidate,
            &round.binding.head,
            options,
            &test.command,
            &directory.join(format!("test-{index}.log")),
            || Ok(false),
        )?;
        proof.tests.push(TestReceipt {
            command: receipt.command,
            commit: receipt.commit,
            exit_code: receipt.exit_code,
            success: receipt.success && !changed,
            output: receipt.output,
        });
    }
    // Retain proof and logs as exact evidence. Native model/tool
    // processes never receive write access to these private controller files.
    super::super::source_store::save(&directory, "proof.json", &serde_json::to_value(&proof)?)?;
    Ok(proof)
}
pub(super) fn options(worker: &str) -> Result<LaunchOptions> {
    let saved = crate::session_policy::saved(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?;
    Ok(LaunchOptions {
        online: saved.is_some_and(|p| p.policy.ends_with("-online")),
        yolo: false,
    })
}
pub(super) fn trusted_candidate(
    state: &super::super::state::State,
) -> Result<kah_run::worktree::Worktree> {
    let id = state
        .worker
        .project
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|s| s.to_str())
        .filter(|id| id.starts_with("run-"))
        .ok_or("REVIEW_FOLLOWUP_CANDIDATE_SCOPE stage=verification")?;
    let base = kah_core::run_store::RunStore::default_state_root()?.join("runs");
    let record = kah_run::store::Store::read(&base, id)?;
    if record.worktree != state.worker.project
        || record.native.as_ref().and_then(|n| n.session.as_deref()) != Some(&state.worker.session)
    {
        return Err("REVIEW_FOLLOWUP_CANDIDATE_SCOPE stage=verification".into());
    }
    // The first completion check already imported this agent-owned SHA through
    // the fsck boundary. Fresh verification selects round.binding.head from
    // that private object store, never whatever HEAD the worker now has.
    Ok(kah_run::worktree::Worktree::load(
        &kah_run::store::Store::path(&base, id)?.join("candidate.json"),
    )?)
}
