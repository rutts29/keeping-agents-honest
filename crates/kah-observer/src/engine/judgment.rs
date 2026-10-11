use super::*;
use crate::judge::{self, Judgment};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn record_usage(state: &mut State, judgment: &Judgment) {
    state.usage.calls += judgment.metrics.calls;
    state.usage.input_tokens += judgment.metrics.input_tokens;
    state.usage.cached_input_tokens += judgment.metrics.cached_input_tokens;
    state.usage.output_tokens += judgment.metrics.output_tokens;
    state.usage.elapsed_ms += judgment.metrics.elapsed_ms;
    state.usage.incomplete_calls += judgment.metrics.failed_calls;
}

pub(super) fn evaluate(
    state: &mut State,
    store: &mut Store,
    input: &Value,
    questions: &judge::Questions,
    purpose: &str,
) -> Result<Judgment, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("JUDGE_HOME stage=observe")?;
    if super::context::plan(state, input, questions) {
        super::context::stage(
            store,
            input,
            questions,
            purpose,
            "usage-calibrated token planning",
        )?;
    }
    let result = judge::routing::evaluate(
        &home,
        state.binding.judge,
        &state.binding.jev_model,
        input,
        questions,
    );
    if result
        .as_ref()
        .is_err_and(|e| e.code == "JEV_CONTEXT_LIMIT")
    {
        super::context::stage(
            store,
            input,
            questions,
            purpose,
            "provider context rejection",
        )?;
    }
    let judgment = result.map_err(|e| {
        state.usage.incomplete_calls += 1;
        e.to_string()
    })?;
    record_usage(state, &judgment);
    let digest = format!("sha256:{:x}", Sha256::digest(input.to_string().as_bytes()));
    store.append(&json!({"kind":"judgment","source":"observer","purpose":purpose,"question_set":judge::questions::VERSION,"state_digest":digest,"judgment":judgment,"at_ms":crate::session::now_ms()}))?;
    state.judgments.push(judgment.clone());
    Ok(judgment)
}
