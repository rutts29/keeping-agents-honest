//! Prepare in input order; only immutable ready bodies enter the concurrent sender.
use super::*;
#[derive(Clone)]
pub(crate) struct Ready {
    pub request: Request,
    pub input: Value,
    pub questions: Questions,
    pub cached: Option<judge::Judgment>,
    selected: Option<(Job, Manifest)>,
}
pub(crate) fn prepare(
    request: &Request,
    input: &Value,
    questions: &Questions,
    calibration: Option<&kah_observer::context_budget::Calibration>,
) -> Result<Ready> {
    if let Some(job) = super::super::state::read::<Job>(&job_path(&request.worker, &request.id)?)? {
        if !matches_decision(&job, input, questions)? {
            archive_changed(request)?;
            stage(
                request,
                input,
                questions,
                "The report/decision changed. Select whole relevant items for the current state and questions; the prior selection and judgment are archived, not reused.",
            )?;
            return Err("JEV_PREPARATION_PENDING stage=context_preparation".into());
        }
        return selected(request, job);
    }
    if let Some(plan) = calibration
        .and_then(|c| c.plan(input, questions))
        .filter(|p| p.prepare)
    {
        stage(request, input, questions, &serde_json::to_string(&plan)?)?;
        return Err("JEV_PREPARATION_PENDING stage=context_preparation".into());
    }
    Ok(Ready {
        request: request.clone(),
        input: input.clone(),
        questions: questions.clone(),
        cached: None,
        selected: None,
    })
}
fn selected(request: &Request, job: Job) -> Result<Ready> {
    let selected = selection_path(&request.worker, &request.id)?;
    if !selected.exists() {
        return Err("JEV_PREPARATION_PENDING stage=context_preparation".into());
    }
    let manifest: Manifest =
        serde_json::from_slice(&super::super::consume::read_report(&selected)?)?;
    let prepared = context_selection::prepare(
        &job.decision,
        &job.state,
        &job.items,
        &manifest,
        &BTreeMap::new(),
    )?;
    if rejection::known(request, &prepared, &job.questions)? {
        return Err(format!("PREPARATION_CONTEXT_UNCONFIRMED stage=context_preparation: {} repeated identical rejected context",request.id).into());
    }
    let path = Store::directory(&request.worker)?.join(format!("prepared-{}.json", request.id));
    let saved: Option<Value> = super::super::state::read(&path)?;
    if saved.as_ref().is_some_and(|s| {
        s["selection"] != serde_json::to_value(&manifest).unwrap()
            || s["prepared_state"] != prepared
    }) {
        return Err("PREPARATION_COMPLETED_IDENTITY stage=context_preparation".into());
    }
    let cached = saved
        .map(|s| serde_json::from_value(s["judgment"].clone()))
        .transpose()?;
    let questions = job.questions.clone();
    Ok(Ready {
        request: request.clone(),
        input: prepared,
        questions,
        cached,
        selected: Some((job, manifest)),
    })
}
pub(crate) fn finish(
    ready: &Ready,
    result: std::result::Result<judge::Judgment, judge::Error>,
) -> Result<(judge::Judgment, Value)> {
    let judgment = match result {
        Ok(judgment) => judgment,
        Err(error) if error.code == "JEV_CONTEXT_LIMIT" => {
            return rejected(ready, &error.to_string());
        }
        Err(error) => return Err(error.into()),
    };
    if let Some((job, manifest)) = &ready.selected {
        archive(
            &ready.request.worker,
            job,
            manifest,
            &ready.input,
            &judgment,
        )?;
    }
    calibrate(
        &ready.request.worker,
        &ready.input,
        &ready.questions,
        &judgment,
    )?;
    Ok((judgment, ready.input.clone()))
}
fn rejected(ready: &Ready, error: &str) -> Result<(judge::Judgment, Value)> {
    if let Some((job, manifest)) = &ready.selected {
        if rejection::record(&ready.request, job, manifest, &ready.input, error)? {
            return Err(format!("PREPARATION_CONTEXT_UNCONFIRMED stage=context_preparation: {} repeated identical rejected context",ready.request.id).into());
        }
        revise(&ready.request, job, error)?;
    } else {
        stage(&ready.request, &ready.input, &ready.questions, error)?;
    }
    Err("JEV_PREPARATION_PENDING stage=context_preparation".into())
}
pub(crate) fn record_retry(ready: &Ready) -> Result<()> {
    if ready.selected.is_some() {
        super::super::source_store::save(
            &Store::directory(&ready.request.worker)?,
            &format!("preparation-retry-{}.json", ready.request.id),
            &json!({"decision":ready.request.id,"selection":ready.selected.as_ref().map(|(_,manifest)|manifest),"prepared_state":ready.input,"questions":ready.questions,"at_ms":kah_observer::session::now_ms()}),
        )?;
    }
    Ok(())
}
pub(crate) fn evaluated(home: &Path, ready: Ready) -> Result<(judge::Judgment, Value)> {
    if let Some(judgment) = ready.cached.clone() {
        return Ok((judgment, ready.input));
    }
    let review = super::cursors::review(&ready.request)?;
    let mut responses = super::send::send(
        home,
        &ready.request.worker,
        &review,
        std::slice::from_ref(&ready),
    )?;
    let response = responses
        .pop()
        .ok_or("JEV_BATCH_BINDING stage=context_preparation")?;
    let result = response.judgment(response.elapsed_ms);
    finish(&ready, result)
}
