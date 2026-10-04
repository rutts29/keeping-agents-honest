use super::state::Request;
use crate::Result;
use kah_observer::{checkpoint, judge::Judgment};
use serde_json::{Value, json};
use std::path::Path;

pub fn evaluate(
    home: &Path,
    request: &Request,
    actions: &[Value],
) -> Result<(Judgment, Value, Value)> {
    let deferrals = super::deferrals::collect(request, actions)?;
    let mut questions = checkpoint::questions();
    if super::checkpoint_claim::doubtful(request) {
        questions.insert("claim_made".into(), super::checkpoint_claim::question());
    }
    for item in &deferrals {
        let id = item["id"]
            .as_str()
            .ok_or("REVIEW_DEFERRAL_ID stage=checkpoint")?;
        questions.extend(checkpoint::deferral_questions(id));
    }
    let mut input = decision_context(
        &request.intent,
        &request.trigger,
        request.head.as_deref(),
        &json!(actions),
        json!(deferrals),
    );
    crate::privacy::scrub(&mut input);
    let binding = kah_observer::session::Store::read(
        &kah_core::run_store::RunStore::default_state_root()?,
        &request.worker,
    )?
    .ok_or("REVIEW_WORKER_STATE stage=checkpoint")?
    .binding;
    if super::preparation::pending(&request.worker, &request.id)? {
        super::preparation::accept(&request.worker, &request.id)?;
    }
    let (prepared_judgment, prepared_input) =
        super::preparation::evaluate(home, request, &input, &questions)?;
    let captured =
        crate::completion_capture::Request::new(&binding.jev_model, &prepared_input, &questions)?;
    let evaluated = Ok(prepared_judgment);
    let archive = crate::completion_capture::save_versioned(
        home,
        &request.worker,
        request.head.as_deref().unwrap_or("uncommitted"),
        "checkpoint",
        &captured,
        &evaluated,
        checkpoint::VERSION,
    )?;
    let judgment = evaluated?;
    if let Some(assessment) = super::checkpoint_claim::assessment(&judgment)? {
        return Ok((judgment, prepared_input, assessment));
    }
    require_coverage(actions)?;
    let mut assessment = assess(&judgment, deferrals);
    assessment["private_replay"] = serde_json::to_value(archive)?;
    if super::checkpoint_claim::doubtful(request) {
        assessment["claim_question_set"] = super::claims::VERSION.into();
    }
    Ok((judgment, prepared_input, assessment))
}
pub fn require_coverage(actions: &[Value]) -> Result<()> {
    for topic in ["intent", "integration", "drift"] {
        if !actions.iter().any(|a| a["evidence"]["topic"] == topic) {
            return Err(format!("REVIEW_CHECKPOINT_EVIDENCE_MISSING stage=checkpoint: provide a {topic} finding with complete exact source quotes for the CURRENT claimed phase and commit; the same request remains pending").into());
        }
    }
    if !actions.iter().any(|a| {
        a["evidence"]["topic"] == "integration" && a["evidence"]["checkpoint_scope"].is_object()
    }) {
        return Err("REVIEW_CHECKPOINT_SCOPE_MISSING stage=checkpoint: name the current phase and component list with a verified authorization citation; the same request remains pending".into());
    }
    Ok(())
}
pub fn assess(judgment: &Judgment, deferrals: Vec<Value>) -> Value {
    let ids = deferrals
        .iter()
        .map(|d| d["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    let outcome = checkpoint::decide_with_deferrals(judgment, &ids);
    let mut open = vec![];
    for item in deferrals {
        let resolved = checkpoint::closed(judgment, item["id"].as_str().unwrap());
        if !resolved {
            open.push(item);
        }
    }
    json!({"state":if outcome.passed {"checkpoint_passed"} else {"checkpoint_failed"},"passed":outcome.passed,"unproven":outcome.unproven,"deferrals":open,"checkpoint_question_set":checkpoint::VERSION})
}

pub fn decision_context(
    intent: &Value,
    trigger: &Value,
    head: Option<&str>,
    actions: &Value,
    deferrals: Value,
) -> Value {
    let mut authority = intent.clone();
    if let Some(fields) = authority.as_object_mut() {
        // Full history stays in the brain's source packet. These telemetry copies
        // were not selected by it, and contradicted current verified evidence.
        fields.remove("failures");
        fields.remove("open_deferrals");
    }
    let mut state = json!({"intent":authority,"event":{"head":head,"trigger":trigger,"actions":actions,"open_and_proposed_deferrals":deferrals}});
    if let Some(scope) = actions.as_array().and_then(|items| {
        items
            .iter()
            .filter(|a| a["evidence"]["topic"] == "integration")
            .map(|a| &a["evidence"]["checkpoint_scope"])
            .find(|s| s.is_object())
    }) {
        state["event"]["checkpoint_scope"] = scope.clone();
    }
    state
}
pub fn record(
    store: &super::state::Store,
    state: &mut super::state::State,
    request: &Request,
    result: &Value,
) -> Result<()> {
    if result["assessment"]["state"] == "checkpoint_not_claimed" {
        return Ok(());
    }
    state.checkpoint = Some(
        json!({"head":request.head,"intent_revision":request.intent["revision"],"request":request.id,"state":result["assessment"]["state"],"passed":result["assessment"]["passed"],"unproven":result["assessment"]["unproven"],"needs_user":result["assessment"]["needs_user"],"needs_thinker":result["assessment"]["needs_thinker"],"at_ms":kah_observer::session::now_ms()}),
    );
    if let Some(event) = request.trigger["events"]
        .as_array()
        .and_then(|events| events.iter().find(|e| e["kind"] == "run_completion"))
    {
        state.checkpoint.as_mut().unwrap()["completion_key"] = event["completion_key"].clone();
    }
    state.deferrals = result["assessment"]["deferrals"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if result["assessment"]["passed"] == false {
        let text = format!(
            "[kah checkpoint review; context, not new user steering] Checkpoint on {} is not established: {}. Fix or demonstrate cumulative integration before starting the next phase. Verified evidence is in review {}.",
            request.head.as_deref().unwrap_or("an uncommitted tree"),
            result["assessment"]["unproven"],
            request.id
        );
        super::reminder_queue::save(
            &store.directory,
            &json!({"request":request.id,"purpose":"checkpoint","turn":request.turn,"text":text}),
        )?;
    }
    Ok(())
}
pub fn notify(state: &super::state::State, request: &Request, result: &Value) -> Result<()> {
    if result["assessment"]["passed"] != false
        || result["assessment"]["state"] == "checkpoint_not_claimed"
    {
        return Ok(());
    }
    let (socket, _) = super::runtime::transport(state)?;
    crate::run_pane::invoke_at(
        &[
            "notification",
            "show",
            "kah checkpoint needs work",
            "--body",
            &format!(
                "Worker {}: {}. Review {}; kah observer {}",
                request.worker, result["assessment"]["unproven"], request.id, request.worker
            ),
            "--sound",
            "request",
        ],
        Some(socket),
    )?;
    Ok(())
}

pub fn covers(state: &super::state::State, head: &str, revision: u64) -> Option<bool> {
    if state.pending.is_some() {
        return None;
    }
    let checkpoint = state.checkpoint.as_ref()?;
    (checkpoint["head"] == head && checkpoint["intent_revision"] == revision)
        .then(|| checkpoint["passed"].as_bool())
        .flatten()
}
