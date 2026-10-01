//! Independent completion judgments for imported native-session candidates.
use kah_core::projection::ActiveProjection;
use kah_observer::{
    judge::{self, Judgment},
    session::Binding,
};
use kah_run::verification::Receipt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub test: String,
    pub observer: Binding,
    // Read old saved settings without keeping the retired fetch execution path.
    #[serde(default, skip_serializing)]
    pub allow_hosts: Vec<String>,
}

pub struct Completion {
    pub complete: bool,
    pub judgments: Vec<Judgment>,
}
pub fn complete_candidate(
    home: &Path,
    settings: &Settings,
    projection: &ActiveProjection,
    actions: &[Value],
    candidate: &kah_run::worktree::Worktree,
    tests: &Receipt,
    online: bool,
) -> Result<Completion, String> {
    let evidence = candidate_evidence(actions, candidate, tests, online)?;
    judge_completion(home, settings, projection, &evidence, tests)
}

/// The same bounded evidence can be inspected/replayed by fixture diagnostics
/// without another native session or an extra judge call.
pub fn candidate_evidence(
    actions: &[Value],
    candidate: &kah_run::worktree::Worktree,
    tests: &Receipt,
    online: bool,
) -> Result<Value, String> {
    Ok(json!({
        "evaluation_scope":"Only the imported candidate commit is delivered. Native agent text and tool outputs are observations, not proof of completion.",
        "observed_activity":actions,
        "git_diff":candidate.diff_evidence(24 * 1024)?,"committed_files":candidate.file_evidence()?,"tests":tests,
        "controller_facts":{"changed_paths":candidate.changed_paths()?,"changed_paths_complete":true,"git_diff_complete":true,"verification_started_from_fresh_checkout_of_commit":tests.commit,"verification_left_candidate_unchanged":true,"candidate_imported_into_trusted_git_with_fsck":true},
        "enforced_policy":{"command_execution":"gVisor","workspace_online":online,"model_login":"host only","native_capabilities":"full","source_repository_writable_mount":false}
    }))
}

fn judge_completion(
    home: &Path,
    settings: &Settings,
    projection: &ActiveProjection,
    evidence: &Value,
    tests: &Receipt,
) -> Result<Completion, String> {
    if projection.needs_review() {
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
    let intent = json!({"goal":projection.goal.text,"outcomes":outcomes,"user_confirmed_ids":projection.frontier.iter().filter(|i|i.user_confirmed).map(|i|i.id.to_string()).collect::<Vec<_>>()});
    let inputs = crate::completion_inputs::states(&intent, evidence)?;
    let mut judgments = vec![];
    for (channel, input) in ["activity", "artifact"].into_iter().zip(inputs) {
        judgments.push(evaluate_completion(
            home,
            &settings.observer,
            &input,
            &questions,
            &tests.commit,
            channel,
        )?);
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

fn evaluate_completion(
    home: &Path,
    binding: &Binding,
    input: &Value,
    questions: &judge::Questions,
    commit: &str,
    channel: &str,
) -> Result<Judgment, String> {
    let request = crate::completion_capture::Request::new(&binding.jev_model, input, questions)?;
    let result = judge::routing::evaluate(
        home,
        binding.judge,
        request.model(),
        request.state(),
        request.questions(),
    );
    let replay = crate::completion_capture::save(
        home,
        &binding.session,
        commit,
        channel,
        &request,
        &result,
    )?;
    record_completion(binding, request.state(), commit, channel, &replay, &result)?;
    result.map_err(|e| e.to_string())
}

fn record_completion(
    binding: &Binding,
    input: &Value,
    commit: &str,
    channel: &str,
    replay: &Path,
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
            store.append(&json!({"kind":"judgment","source":"completion","channel":channel,"replay":replay,"question_set":judge::questions::VERSION,"state_digest":digest,"commit":commit,"judgment":judgment,"assessment":{"state":"completion"}}))?;
        }
        Err(error) => {
            state.usage.incomplete_calls += 1;
            state.fail(&error.code, error.to_string());
            store.append(&json!({"kind":"gap","source":"completion","channel":channel,"replay":replay,"gap":state.gap}))?;
        }
    }
    store.save(&state)
}
