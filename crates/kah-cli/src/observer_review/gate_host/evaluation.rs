//! The driver lease is per gate; no main Store reaches the evaluator.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct Evaluation {
    pub judgment: Judgment,
    pub input: Value,
    pub assessment: Value,
}
pub fn run(worker: &str) -> Result<()> {
    super::super::runtime::require_active(worker)?;
    if super::super::service::blocked(worker)? {
        return Ok(());
    }
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=gate_host")?);
    for root in jobs(worker)? {
        if !ready(&root)? {
            continue;
        }
        let job: Job = source_store::read(&root.join("job.json"))?;
        let result = process_with(&job, |job| {
            let (judgment, input, assessment) = super::super::consume::gate_decision(
                &home,
                &job.request,
                &job.actions,
                &job.report,
            )?;
            Ok(Evaluation {
                judgment,
                input,
                assessment,
            })
        });
        match result {
            Ok(result) => {
                let state = Store::read(worker)?.ok_or("REVIEW_STATE_MISSING stage=gate_host")?;
                super::super::checkpoint::notify(&state, &job.request, &result)?;
                if job.submitted.is_file() {
                    fs::remove_file(&job.submitted)?;
                }
                super::super::queue::kick(worker)?;
            }
            Err(e) if e.to_string().starts_with("REVIEW_BUSY") => (),
            Err(e) => retain_failure(&root, &job, e.as_ref())?,
        }
    }
    Ok(())
}
/// Owns only this job's OS lease. A process exit releases it, retaining caches.
pub(crate) fn process_with(
    job: &Job,
    evaluate: impl FnOnce(&Job) -> Result<Evaluation>,
) -> Result<Value> {
    let root = directory(&job.request.worker, &job.request.id)?;
    let _driver = Store::delivery_at(&root.join("driver"), false)?;
    if let Some(result) = super::super::state::read(&root.join("finished.json"))? {
        return Ok(result);
    }
    resume_handoff(job)?;
    let _processing = super::super::liveness::processing(&job.request.worker, &job.request.id)?;
    let evaluated = evaluate_unlocked(job, evaluate)?;
    merge_fresh(
        job,
        &evaluated.judgment,
        &evaluated.input,
        &evaluated.assessment,
    )
}
/// Jev receives the immutable job, with no held main store lock or mutable State.
fn evaluate_unlocked(
    job: &Job,
    evaluate: impl FnOnce(&Job) -> Result<Evaluation>,
) -> Result<Evaluation> {
    let root = directory(&job.request.worker, &job.request.id)?;
    if let Some(saved) = super::super::state::read(&root.join("evaluated.json"))? {
        return Ok(saved);
    }
    let evaluated = evaluate(job)?;
    source_store::save(&root, "evaluated.json", &serde_json::to_value(&evaluated)?)?;
    Ok(evaluated)
}
/// Recover a crash between saving job.json and releasing the native slot.
fn resume_handoff(job: &Job) -> Result<()> {
    let store = Store::open_wait(&job.request.worker)?;
    let mut state = store
        .load()?
        .ok_or("REVIEW_STATE_MISSING stage=gate_handoff")?;
    if state.observer.session != job.state.observer.session {
        return Err("REVIEW_GATE_HOST_OBSERVER stage=gate_handoff".into());
    }
    if state
        .pending
        .as_ref()
        .is_some_and(|p| p.id == job.request.id)
    {
        if !super::super::liveness::handoff::awaiting_host(&state)? {
            return Err("REVIEW_GATE_HOST_ACK stage=gate_handoff".into());
        }
        state.pending = None;
        store.save(&state)?;
    }
    Ok(())
}
fn retain_failure(root: &Path, job: &Job, error: &(dyn std::error::Error + 'static)) -> Result<()> {
    let failures = root.join("failures");
    kah_core::run_store::ensure_private_directory(&failures)?;
    crate::unattended_report::save(
        &failures,
        &format!("{}.json", crate::relay_store::nonce()),
        &json!({"request":job.request.id,"error":error.to_string()}),
    )?;
    if !error.to_string().starts_with("JEV_PREPARATION_PENDING") {
        return marker(
            root,
            "stage.json",
            if kah_observer::judge::availability::outage(&error.to_string()) {
                "evaluating" // Service recovery resumes the same per-decision caches.
            } else {
                "failed"
            },
        );
    }
    marker(root, "stage.json", "needs_native_context")?;
    super::super::queue::enqueue_once_at(
        &Store::directory(&job.request.worker)?,
        &format!(
            "gate-context:{}:{}",
            job.request.id,
            super::super::preparation::active_decision(&job.request.worker, &job.request.id)?
        ),
        &super::super::queue::Wake {
            purpose: job.request.purpose.clone(),
            turn: job.request.turn.clone(),
            trigger: json!({"captured_snapshot":job.request,"gate_host_continuation":true}),
        },
    )?;
    super::super::queue::kick(&job.request.worker)
}
