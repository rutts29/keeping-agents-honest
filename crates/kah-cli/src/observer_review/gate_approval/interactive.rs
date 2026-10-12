//! Interactive explicit gates reuse controller-owned bundle/check boundaries.
use super::super::{gate_host::Job, source_store, state::Store};
use super::*;
use kah_observer::decision_units::artifact::Receipt;
use std::{collections::BTreeSet, fs, path::PathBuf};
mod candidate;
mod missing;
pub use candidate::bundle_cli;
pub use missing::guard;
pub(super) use missing::missing_question;

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
    let commands = recorded_commands(&job.request, &job.report)?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=verification")?);
    let tree = candidate::load(root, &job.state, head)?;
    if commands.is_empty() {
        inventory(&tree, head)?;
    }
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
    let options = super::checks::options(&job.request.worker)?;
    let proof = verify_commands(head, &commands, artifacts, |index, command| {
        recorded_check(
            &home,
            &tree,
            head,
            options,
            command,
            &root.join(format!("check-{index}.log")),
        )
    })?;
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
fn recorded_check(
    home: &Path,
    tree: &kah_run::worktree::Worktree,
    head: &str,
    options: kah_sandbox::LaunchOptions,
    command: &str,
    log: &Path,
) -> Result<TestReceipt> {
    if log.with_extension("reserved").exists()
        && !crate::exact_verification::receipt_path(log).exists()
    {
        return Err("REVIEW_FOLLOWUP_CHECK_UNKNOWN stage=verification: recorded check was reserved without a receipt; retained, never silently repeated".into());
    }
    if let Some(saved) =
        super::super::state::read::<Value>(&crate::exact_verification::receipt_path(log))?
    {
        return Ok(test_receipt(
            serde_json::from_value(saved["receipt"].clone())?,
            saved["changed"] == true,
        ));
    }
    fs::write(log.with_extension("reserved"), command)?;
    let (receipt, changed) =
        crate::exact_verification::execute(home, tree, head, options, command, log, || Ok(false))?;
    Ok(test_receipt(receipt, changed))
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
fn test_receipt(receipt: kah_run::verification::Receipt, changed: bool) -> TestReceipt {
    TestReceipt {
        command: receipt.command,
        commit: receipt.commit,
        exit_code: receipt.exit_code,
        success: receipt.success && !changed,
        output: receipt.output,
    }
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
fn recorded_commands(request: &Request, report: &Report) -> Result<Vec<String>> {
    let verified = super::super::decisions::verify_evidence(request, report)?
        .ok_or("REVIEW_GATE_CHECK_SOURCE stage=verification")?;
    let mut seen = BTreeSet::new();
    let mut commands = vec![];
    for r in report.decision_units.iter().flat_map(|u| &u.requirements) {
        let value = verified["requirements"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == r.id)
            .ok_or("REVIEW_GATE_CHECK_SOURCE stage=verification")?;
        for check in &r.checks {
            let records = &value["citations"][check.command_citation]["native_records"];
            let recorded = records
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r["executions"].as_array().into_iter().flatten())
                .filter_map(|e| e["command"].as_str())
                .collect::<BTreeSet<_>>();
            if recorded.len() != 1 {
                return Err("REVIEW_GATE_CHECK_SOURCE stage=verification: cite exactly one whole recorded native command, not an ambiguous range".into());
            }
            let command = recorded.into_iter().next().unwrap();
            if seen.insert(command.to_owned()) {
                commands.push(command.to_owned());
            }
        }
    }
    Ok(commands)
}
