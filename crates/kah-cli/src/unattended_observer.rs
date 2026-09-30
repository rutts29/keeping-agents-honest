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
    tests: &Receipt,
) -> Result<Completion, String> {
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
    let evidence = json!({"native_actions":actions,"git_diff":diff,"tests":tests,
        "enforced_policy":{"gvisor":true,"network":"none","credentials":"not mounted","git_metadata":"read only"}});
    let input = json!({"intent":{"outcomes":outcomes},"event":{"actions":evidence}});
    let mut judgments = vec![];
    for _ in 0..2 {
        judgments.push(
            judge::routing::evaluate(
                home,
                settings.observer.judge,
                &settings.observer.jev_model,
                &input,
                &questions,
            )
            .map_err(|e| e.to_string())?,
        );
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
