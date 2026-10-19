//! Detached controller follow-up; builder replies remain evidence, not authority.
use super::super::{hook, source_store, state::Store};
use super::{followup::*, *};
use std::{collections::BTreeSet, fs};
mod builder;
mod conclusion;
pub(crate) use conclusion::{finish, guard};

#[derive(Serialize, Deserialize)]
struct Job {
    request: String,
    proof: DeliveryProof,
    assessment: Value,
    report: Report,
}
fn directory(root: &Path, round: &VerificationRound) -> std::path::PathBuf {
    root.join("gate-followups").join(&round.id)
}
pub(crate) fn begin(
    store: &Store,
    request: &Request,
    report: &Report,
    assessment: &Value,
) -> Result<bool> {
    begin_with(store, request, report, assessment, || {
        hook::spawn("__gate-followup", &request.worker)
    })
}
pub(super) fn begin_with(
    store: &Store,
    request: &Request,
    report: &Report,
    assessment: &Value,
    schedule: impl FnOnce() -> Result<()>,
) -> Result<bool> {
    let Some(proof) = captured(request)? else {
        return Ok(false);
    };
    if (!request.trigger["reconsideration"].is_object()
        && report
            .decision_units
            .as_ref()
            .is_none_or(|u| u.version() < 12))
        || !positive_uncertainty(request, report, assessment, &proof)
    {
        return Ok(false);
    }
    let round = start(&store.directory, request)?;
    if round.stage == Stage::Complete {
        return Ok(false);
    }
    let root = directory(&store.directory, &round);
    if root.join("finished.json").exists() {
        super::contract_delivery::retry(&store.directory, request, &round)?;
        let finished: Value = source_store::read(&root.join("finished.json"))?;
        if finished["status"] == "needs_check" {
            return Ok(true);
        }
    }
    kah_core::run_store::ensure_private_directory(&root)?;
    if !root.join("job.json").exists() {
        source_store::save(
            &root,
            "job.json",
            &serde_json::to_value(Job {
                request: request.id.clone(),
                proof,
                assessment: assessment.clone(),
                report: report.clone(),
            })?,
        )?;
    }
    if round.stage == Stage::Checks && root.join("checks.json").is_file() {
        let state = store
            .load()?
            .ok_or("REVIEW_STATE_MISSING stage=verification")?;
        let job: Job = source_store::read(&root.join("job.json"))?;
        return schedule_checks(&store.directory, &root, &state, &round, request, &job)
            .map(|_| true);
    }
    if round.stage == Stage::Evidence {
        let evidence = selected_evidence(request, report)?;
        advance(&store.directory, &round.binding, Step::Evidence(evidence))?;
    }
    schedule()?;
    Ok(true)
}
pub fn pending(worker: &str) -> Result<bool> {
    Ok(!jobs(&Store::directory(worker)?)?.is_empty())
}
fn jobs(root: &Path) -> Result<Vec<std::path::PathBuf>> {
    let directory = root.join("gate-followups");
    if !directory.is_dir() {
        return Ok(vec![]);
    }
    let mut paths = fs::read_dir(directory)?
        .map(|e| e.map(|e| e.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|p| {
        p.is_dir() && p.join("job.json").is_file() && !p.join("finished.json").exists()
    });
    paths.sort();
    Ok(paths)
}
/// A round begun after this job listed its jobs, or builder progress that the
/// controller saw while it ran, starts no job (finding #21).
pub fn run(worker: &str) -> Result<()> {
    let lease = super::super::job_lease::lease(worker, "__gate-followup")?;
    run_with(worker, &lease, || pass(worker))
}
pub fn run_with(worker: &str, lease: &fs::File, pass: impl FnMut() -> Result<()>) -> Result<()> {
    super::super::job_lease::until_requested(worker, "__gate-followup", lease, pass)
}
fn pass(worker: &str) -> Result<()> {
    super::super::runtime::require_active(worker)?;
    let root = Store::directory(worker)?;
    let _driver = Store::delivery_at(&root.join("gate-followup-driver"), true)?;
    for path in jobs(&root)? {
        if let Err(error) = drive_one(&root, &path, worker) {
            failure(&root, &path, worker, &error.to_string())?;
        }
    }
    Ok(())
}

fn selected_evidence(request: &Request, report: &Report) -> Result<Vec<EvidencePointer>> {
    let sources = report
        .findings
        .iter()
        .flat_map(|f| &f.citations)
        .chain(
            report
                .decision_units
                .iter()
                .flat_map(|u| &u.requirements)
                .flat_map(|r| &r.citations),
        )
        .map(|c| c.source.as_str())
        .collect::<BTreeSet<_>>();
    let evidence = sources
        .into_iter()
        .map(|id| {
            let document = request
                .documents
                .iter()
                .find(|d| d.id == id)
                .ok_or("REVIEW_FOLLOWUP_SOURCE stage=verification")?;
            Ok(EvidencePointer {
                receipt: id.into(),
                record: document.uri.clone(),
                digest: document.digest.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(evidence)
}

fn schedule_checks(
    root: &Path,
    path: &Path,
    state: &super::super::state::State,
    round: &VerificationRound,
    request: &Request,
    job: &Job,
) -> Result<()> {
    let mut proof = if let Some(proof) =
        super::super::state::read::<DeliveryProof>(&path.join("checks.json"))?
    {
        proof
    } else if claim(root, &round.binding, Stage::Checks)? {
        let proof = super::checks::execute(root, state, round, &job.proof)?;
        source_store::save(path, "checks.json", &serde_json::to_value(&proof)?)?;
        proof
    } else {
        return Err("REVIEW_FOLLOWUP_CHECK_UNKNOWN stage=verification: reserved attempt has no complete receipt; retained visibly, never rerun silently".into());
    };
    let items = super::contract_execution::execute(
        root,
        state,
        round,
        &job.report,
        &job.assessment,
        &proof,
        request,
    )?;
    append_planned_checks(&mut proof, &items);
    source_store::save(path, "checks.json", &serde_json::to_value(&proof)?)?;
    source_store::save(path, "item-results.json", &serde_json::to_value(items)?)?;
    queue_checks(root, path, round, request, job, &proof)?;
    source_store::save(
        path,
        "finished.json",
        &json!({"status":"awaiting_thinker","request":request.id}),
    )?;
    hook::spawn("__review-wake", &request.worker)
}

fn append_planned_checks(
    proof: &mut DeliveryProof,
    items: &[super::contract_execution::ItemResult],
) {
    for check in items.iter().filter_map(|i| i.check.as_ref()) {
        if !proof
            .tests
            .iter()
            .any(|t| t.command == check.command && t.commit == check.commit)
        {
            proof.tests.push(check.clone());
        }
    }
    proof.recorded_checks_absent = proof.tests.is_empty();
}

fn drive_one(root: &Path, path: &Path, worker: &str) -> Result<()> {
    let job: Job = source_store::read(&path.join("job.json"))?;
    let request: Request =
        source_store::read(&root.join("requests").join(format!("{}.json", job.request)))?;
    let mut round = start(root, &request)?;
    let state = Store::read(worker)?.ok_or("REVIEW_STATE_MISSING stage=verification")?;
    if round.stage == Stage::Builder {
        if let Some(receipt) = builder::attempt(root, &state, &round, &request, &job)? {
            round = advance(root, &round.binding, Step::Builder(receipt))?;
        } else {
            return Ok(());
        }
    }
    if round.stage == Stage::Checks {
        schedule_checks(root, path, &state, &round, &request, &job)?;
    }
    Ok(())
}
fn failure(root: &Path, path: &Path, worker: &str, error: &str) -> Result<()> {
    let state = Store::read(worker)?.ok_or("REVIEW_STATE_MISSING stage=verification")?;
    let id = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("followup");
    let previous: Option<Value> = super::super::state::read(&path.join("failure.json"))?;
    if previous
        .as_ref()
        .is_some_and(|p| p["error"] == error && p["reviewer_submitted"] == true)
    {
        return Ok(());
    }
    let text = format!(
        "Verification follow-up {id} for worker {worker} remains pending: {error}. Evidence is retained; the same ask/check is never rerun silently."
    );
    super::super::reminder_queue::save(
        root,
        &json!({"request":id,"purpose":"uncertainty","turn":"verification-followup","system_notice":true,"text":text}),
    )?;
    let delivery = state
        .worker
        .socket
        .as_deref()
        .ok_or("REVIEW_HERDR_REQUIRED stage=notice".into())
        .and_then(|socket| {
            crate::run_pane::invoke_at(
                &[
                    "agent",
                    "prompt",
                    "kah-reviewer",
                    &format!("[kah-builder] [kah-verification-pending] {text}"),
                ],
                Some(socket),
            )
        });
    // Only an externally reserved operation with unknown outcome must park.
    // Read/connect failures keep the same durable job recoverable on new events.
    if error.starts_with("REVIEW_FOLLOWUP_CHECK_UNKNOWN") {
        source_store::save(
            path,
            "finished.json",
            &json!({"status":"host_failed","error":error,"request":state.pending.as_ref().map(|p|&p.id)}),
        )?;
    }
    source_store::save(
        path,
        "failure.json",
        &json!({"error":error,"reviewer_submitted":delivery.is_ok(),"delivery_error":delivery.err().map(|e|e.to_string()),"at_ms":kah_observer::session::now_ms()}),
    )
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/followup_host_failure.rs"]
mod recovery_tests;

fn queue_checks(
    root: &Path,
    path: &Path,
    round: &VerificationRound,
    request: &Request,
    job: &Job,
    proof: &DeliveryProof,
) -> Result<bool> {
    let items: Option<Vec<super::contract_execution::ItemResult>> =
        super::super::state::read(&path.join("item-results.json"))?;
    let mut trigger = request.trigger.clone();
    trigger["verification_followup"] = json!({"round":round.id,"stage":"checks_complete","proof":proof,
        "record":path.join("checks.json"),"builder":round.builder,"original_unconfirmed_items":job.assessment["needs_thinker"],"item_results":items.unwrap_or_default()});
    super::super::queue::enqueue_once_at(
        root,
        &format!("{}-{}", round.checks_receipt(), request.id),
        &super::super::queue::Wake {
            purpose: Purpose::Checkpoint,
            turn: request.turn.clone(),
            trigger,
        },
    )
}

#[cfg(test)]
#[path = "../../../tests/observer_unit/interactive_planned_pass.rs"]
mod planned_pass_tests;
