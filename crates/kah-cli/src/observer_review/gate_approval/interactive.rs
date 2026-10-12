//! Interactive explicit gates reuse controller-owned bundle/check boundaries.
use super::super::{gate_host::Job, source_store, state::Store};
use super::*;
use kah_observer::decision_units::artifact::Receipt;
use std::{fs, path::PathBuf};
mod candidate;
mod missing;
mod replay;
mod risk_text;
pub use candidate::bundle_cli;
pub use missing::guard;
pub(super) use missing::missing_question;
pub(super) use risk_text::derive as risk_text;
pub(super) use risk_text::for_report as report_risk;

pub(super) fn usable_plan(report: &Report) -> bool {
    report
        .decision_units
        .as_ref()
        .is_some_and(|u| u.version() >= 12)
        && report.verification_contracts.as_ref().is_some_and(|items| {
            !items.is_empty()
                && items
                    .iter()
                    .all(|i| !i.claim.trim().is_empty() && !i.coverage_reasoning.trim().is_empty())
        })
}

pub(crate) fn prepare(job: &mut Job) -> Result<()> {
    if job.request.trigger["option1_cumulative_input"].is_null()
        || job.request.trigger["events"]
            .as_array()
            .is_some_and(|e| e.iter().any(|e| e["kind"] == "run_completion"))
    {
        return Ok(());
    }
    let root = Store::directory(&job.request.worker)?
        .join("interactive-gates")
        .join(&job.request.id);
    kah_core::run_store::ensure_private_directory(&root)?;
    let proof = match super::super::state::read::<DeliveryProof>(&root.join("proof.json"))? {
        Some(proof) => proof,
        None => execute(&root, job)?,
    };
    job.request.trigger["interactive_gate_verification"] = json!({"proof":proof,"record":root.join("proof.json"),"scope":"Fresh exact-SHA verification, not an original pre-work preservation baseline"});
    super::super::capture::controller_receipts::add(&mut job.request)?;
    source_store::save(
        &Store::directory(&job.request.worker)?.join("requests"),
        &format!("{}.json", job.request.id),
        &json!(job.request),
    )?;
    Ok(())
}
fn execute(root: &Path, job: &Job) -> Result<DeliveryProof> {
    let head = job
        .request
        .head
        .as_deref()
        .ok_or("REVIEW_GATE_HEAD_MISSING stage=verification")?;
    source_store::save(
        root,
        "recorded-check-dispositions.json",
        &replay::dispositions(&job.request, &job.report)?,
    )?;
    let tree = candidate::load(root, &job.state, head)?;
    inventory(&tree, head)?;
    let index = Store::directory(&job.request.worker)?.join("interactive-candidates");
    source_store::save(
        &index,
        &format!("{head}.json"),
        &json!({"worker":job.request.worker,"head":head,"project":job.state.worker.project,"candidate":root.join("candidate.json")}),
    )?;
    let blobs = super::super::artifact_collect::committed(&tree.path, head)?;
    let artifacts = blobs
        .into_iter()
        .map(|(path, blob)| Receipt {
            path,
            baseline_blob: blob.clone(),
            delivered_blob: Some(blob),
            delivered_sha: head.into(),
        })
        .collect();
    let proof = DeliveryProof {
        head: head.into(),
        fresh_checkout: true,
        tests: vec![],
        artifacts,
        recorded_checks_absent: true,
    };
    source_store::save(root, "proof.json", &json!(proof))?;
    Ok(proof)
}
fn inventory(tree: &kah_run::worktree::Worktree, head: &str) -> Result<()> {
    let snapshot = tree.verification_snapshot(
        &tree
            .path
            .parent()
            .unwrap()
            .join(format!("inventory-{}", crate::relay_store::nonce())),
        head,
    )?;
    let result = if snapshot.head()? == head {
        Ok(())
    } else {
        Err("REVIEW_GATE_CHECK_BINDING stage=verification".into())
    };
    snapshot.remove_verification()?;
    result
}
pub(super) fn candidate_for(
    state: &crate::observer_review::state::State,
    head: &str,
) -> Result<kah_run::worktree::Worktree> {
    let root = Store::directory(&state.worker.session)?.join("interactive-candidates");
    let binding: Value = super::super::state::read(&root.join(format!("{head}.json")))?
        .ok_or("REVIEW_GATE_CANDIDATE_MISSING stage=verification")?;
    if binding["worker"] != state.worker.session
        || binding["head"] != head
        || binding["project"] != json!(state.worker.project)
    {
        return Err("REVIEW_GATE_CANDIDATE_BINDING stage=verification".into());
    }
    let path = PathBuf::from(
        binding["candidate"]
            .as_str()
            .ok_or("REVIEW_GATE_CANDIDATE_BINDING stage=verification")?,
    );
    if !path.starts_with(Store::directory(&state.worker.session)?.join("interactive-gates")) {
        return Err("REVIEW_GATE_CANDIDATE_BINDING stage=verification".into());
    }
    kah_run::worktree::Worktree::load(&path).map_err(Into::into)
}
pub fn verify_commands(
    head: &str,
    commands: &[String],
    artifacts: Vec<Receipt>,
    mut execute: impl FnMut(usize, &str) -> Result<TestReceipt>,
) -> Result<DeliveryProof> {
    let mut tests = vec![];
    for (index, command) in commands.iter().enumerate() {
        let receipt = execute(index, command)?;
        if receipt.commit != head || receipt.command != *command {
            return Err("REVIEW_GATE_CHECK_BINDING stage=verification".into());
        }
        tests.push(receipt);
    }
    Ok(DeliveryProof {
        head: head.into(),
        fresh_checkout: true,
        tests,
        artifacts,
        recorded_checks_absent: commands.is_empty(),
    })
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/candidate_registration.rs"]
mod registration_tests;

#[cfg(test)]
#[path = "../../../tests/observer_unit/interactive_first_round.rs"]
mod first_round_tests;

#[cfg(test)]
#[path = "../../../tests/observer_unit/recorded_replay.rs"]
mod replay_tests;
