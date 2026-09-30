//! The controller supplies operator authority; ephemeral native output is evidence.
use kah_core::{observation::ObservationSnapshot, projection::ActiveProjection};
use kah_observer::{
    engine,
    judge::{self, Judgment},
    session::Binding,
};
use kah_run::{intake, store::Record, verification::Receipt};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub test: String,
    pub observer: Binding,
    #[serde(default)]
    pub allow_hosts: Vec<String>,
}

pub fn observe(
    record: &Record,
    settings: &Settings,
    turn: &str,
    actions: Vec<Value>,
) -> Result<engine::Outcome, String> {
    let users = intake::user_bundle(
        &record.worktree,
        &settings.observer.session,
        &record.control,
    )?;
    let digest = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(&actions).unwrap())
    );
    engine::observe_snapshot(
        &settings.observer,
        turn,
        true,
        ObservationSnapshot {
            users: Some(users),
            actions,
            action_digest: digest,
        },
    )
}

pub fn worker_prompt(
    record: &Record,
    projection: Option<&ActiveProjection>,
    feedback: &str,
) -> String {
    json!({
        "controller_policy": "Work only in this disposable worktree. All tools execute in gVisor without credentials. Network is off. The controller owns Git checkpoints; do not commit, push, deploy or publish. If an external action is required, return external_action_required with a concrete question. Otherwise implement the operator's active task and report ready or continue; only independent tests and the intent judge can complete this run.",
        "operator_inputs": record.control.inputs(),
        "intent_projection": projection.map(ActiveProjection::rendered_prompt),
        "controller_feedback_not_user_steering": feedback,
        "final_response": "Return the required JSON object with status, summary and question. Do not invent successful tests or completion evidence."
    }).to_string()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerReply {
    pub status: String,
    pub summary: String,
    pub question: String,
}
impl WorkerReply {
    pub fn from_actions(actions: &[Value]) -> Result<Self, String> {
        let text = actions
            .iter()
            .rev()
            .find(|v| v["kind"] == "agentMessage")
            .and_then(|v| v["evidence"]["text"].as_str())
            .ok_or("RUN_REPLY_MISSING stage=worker_result")?;
        let reply: Self =
            serde_json::from_str(text).map_err(|_| "RUN_REPLY_JSON stage=worker_result")?;
        if !matches!(
            reply.status.as_str(),
            "ready" | "continue" | "external_action_required"
        ) || reply.summary.len() > 4096
            || reply.question.len() > 4096
            || reply.status == "external_action_required" && reply.question.trim().is_empty()
        {
            return Err("RUN_REPLY_SHAPE stage=worker_result".into());
        }
        Ok(reply)
    }
    pub fn schema() -> Value {
        json!({"type":"object","additionalProperties":false,"required":["status","summary","question"],"properties":{
            "status":{"type":"string","enum":["ready","continue","external_action_required"]},
            "summary":{"type":"string"},"question":{"type":"string"}
        }})
    }
}

pub struct Completion {
    pub complete: bool,
    pub judgments: Vec<Judgment>,
}
pub fn complete(
    home: &Path,
    settings: &Settings,
    projection: &ActiveProjection,
    actions: &[Value],
    diff: &str,
    changed_paths: &[String],
    tests: &Receipt,
) -> Result<Completion, String> {
    if projection
        .preserve
        .iter()
        .chain(&projection.frontier)
        .chain(&projection.validation)
        .any(|i| !i.uncertain_supersedes.is_empty())
    {
        return Err(
            "RUN_INTENT_UNCERTAIN stage=completion: unresolved steering replacement".into(),
        );
    }
    let mut outcomes = vec![("goal".into(), projection.goal.text.clone())];
    outcomes.extend(
        projection
            .preserve
            .iter()
            .chain(&projection.frontier)
            .chain(&projection.validation)
            .map(|item| (item.id.to_string(), item.text.clone())),
    );
    if outcomes.len() > 24 {
        return Err(
            "RUN_OUTCOME_BOUND stage=completion: too many outcomes for one bounded decision".into(),
        );
    }
    let questions = judge::questions::completion(&outcomes);
    let directory = kah_core::run_store::RunStore::default_state_root()
        .map_err(|e| e.to_string())?
        .join("runs")
        .join(&settings.observer.session);
    let files =
        kah_run::worktree::Worktree::load(&directory.join("worktree.json"))?.file_evidence()?;
    let activity = actions.iter().filter(|a| a["kind"] != "agentMessage").map(|a| json!({
        "kind":a["kind"],"command":a["evidence"]["command"],"changes":a["evidence"]["changes"],"exit_code":a["exit_code"],"status":a["status"]
    })).collect::<Vec<_>>();
    let evidence = json!({"evaluation_scope":"The candidate branch delivered for review. Temporary compiler/test files live in a separate verification checkout and are not delivered project changes.","observed_activity":activity,"git_diff":diff,"committed_files":files,"tests":tests,
        "controller_facts":{"changed_paths":changed_paths,"changed_paths_complete":true,"git_diff_complete":true,"verification_started_from_fresh_checkout_of_commit":tests.commit,"verification_left_candidate_unchanged":true},
        "enforced_policy":{"gvisor":true,"container_network":"none","allowed_https_get_hosts":settings.allow_hosts,"credentials":"not mounted","git_metadata":"read only","native_subagent_tools":"disabled","native_goal_tools":"disabled","worker_count":1}});
    let input = json!({"intent":{"goal":projection.goal.text,"outcomes":outcomes},"event":{"actions":evidence}});
    let mut judgments = vec![];
    for _ in 0..2 {
        let result = judge::routing::evaluate(
            home,
            settings.observer.judge,
            &settings.observer.jev_model,
            &input,
            &questions,
        );
        record_completion(&settings.observer, &input, &tests.commit, &result)?;
        judgments.push(result.map_err(|e| e.to_string())?);
    }
    let complete = tests.success
        && judgments
            .iter()
            .all(|result| judge::decision::complete(result, &outcomes));
    Ok(Completion {
        complete,
        judgments,
    })
}

fn record_completion(
    binding: &Binding,
    input: &Value,
    commit: &str,
    result: &Result<Judgment, judge::Error>,
) -> Result<(), String> {
    use kah_core::run_store::RunStore;
    use kah_observer::session::Store;
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let mut store = Store::open(&base, &binding.session, std::time::Duration::from_secs(1))?;
    let mut state = store.load()?.ok_or("RUN_OBSERVER_STATE stage=completion")?;
    match result {
        Ok(judgment) => {
            state.usage.calls += judgment.metrics.calls;
            state.usage.input_tokens += judgment.metrics.input_tokens;
            state.usage.output_tokens += judgment.metrics.output_tokens;
            state.usage.elapsed_ms += judgment.metrics.elapsed_ms;
            state.usage.incomplete_calls += judgment.metrics.failed_calls;
            state.judgments.push(judgment.clone());
            if state.judgments.len() > 16 {
                state.judgments.remove(0);
            }
            let digest = format!("sha256:{:x}", Sha256::digest(input.to_string().as_bytes()));
            store.append(&json!({"kind":"judgment","source":"completion","question_set":judge::questions::VERSION,"state_digest":digest,"commit":commit,"judgment":judgment,"assessment":{"state":"completion"}}))?;
        }
        Err(error) => {
            state.usage.incomplete_calls += 1;
            state.fail(&error.code, error.to_string());
            store.append(&json!({"kind":"gap","source":"completion","gap":state.gap}))?;
        }
    }
    store.save(&state)
}
