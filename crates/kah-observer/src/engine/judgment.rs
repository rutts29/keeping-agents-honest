use super::*;
use crate::judge::{self, Judgment};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub(super) struct Context<'a> {
    pub prior: Option<&'a ActiveProjection>,
    pub projection: &'a ActiveProjection,
    pub users: &'a NeutralSessionBundle,
    pub actions: &'a [Value],
    pub new_user: bool,
    pub turn: &'a str,
}

pub(super) fn assess(
    state: &mut State,
    store: &mut Store,
    context: Context<'_>,
    config: CompactorConfig<'_>,
) -> Result<judge::decision::Assessment, String> {
    let prior_items = context.prior.map(items).unwrap_or_default();
    let questions = judge::questions::observation(&prior_items);
    let input = bounded_state(state, &context, &prior_items);
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("JUDGE_HOME stage=observe")?;
    let judgment = match judge::routing::evaluate(
        &home,
        &state.binding.project,
        state.binding.judge,
        config,
        &input,
        &questions,
    ) {
        Ok(result) => result,
        Err(error) => {
            state.usage.incomplete_calls += 1;
            return Err(error.to_string());
        }
    };
    record_usage(state, &judgment);
    let assessment = judge::decision::assessment(&judgment, context.actions, state.failures.len());
    let digest = format!("sha256:{:x}", Sha256::digest(input.to_string().as_bytes()));
    store.append(&json!({"kind":"judgment","source":"observer","question_set":judge::questions::VERSION,"state_digest":digest,"judgment":judgment,"assessment":assessment,"turn":context.turn,"at_ms":crate::session::now_ms()}))?;
    if let Some(reason) = &judgment.fallback_reason {
        store.append(&json!({"kind":"gap","source":"judge_backend","preferred":"jev","fallback":"luna","reason":reason,"usage_complete":false,"turn":context.turn}))?;
    }
    state.judgments.push(judgment);
    if state.judgments.len() > 16 {
        state.judgments.remove(0);
    }
    record_failures(state, context.actions);
    Ok(assessment)
}

fn bounded_state(state: &State, context: &Context<'_>, prior_items: &[(String, String)]) -> Value {
    let user = context
        .users
        .records()
        .last()
        .filter(|_| context.new_user)
        .map(|r| r.content())
        .unwrap_or("");
    let agent = context
        .actions
        .iter()
        .filter(|a| a["kind"] == "agentMessage")
        .filter_map(|a| a["evidence"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    json!({
        "prior_intent":{"goal":context.prior.map(|p|&p.goal.text),"items":prior_items},
        "intent":{"goal":context.projection.goal.text,"items":items(context.projection),"failures":state.failures,"completed_outcomes":[],"open_outcomes":[]},
        "event":{"user_text":bounded(user,4096),"agent_text":bounded(&agent,4096),"actions":context.actions}
    })
}
fn items(projection: &ActiveProjection) -> Vec<(String, String)> {
    projection
        .preserve
        .iter()
        .chain(&projection.frontier)
        .chain(&projection.validation)
        .map(|i| (i.id.to_string(), i.text.clone()))
        .collect()
}
fn bounded(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}
fn record_usage(state: &mut State, judgment: &Judgment) {
    state.usage.calls += 1;
    state.usage.input_tokens += judgment.metrics.input_tokens;
    state.usage.cached_input_tokens += judgment.metrics.cached_input_tokens;
    state.usage.output_tokens += judgment.metrics.output_tokens;
    state.usage.elapsed_ms += judgment.metrics.elapsed_ms;
    if judgment.fallback_reason.is_some() {
        state.usage.incomplete_calls += 1;
    }
}
fn record_failures(state: &mut State, actions: &[Value]) {
    for action in actions {
        if action["kind"] != "commandExecution" {
            continue;
        }
        // Status survives even when a long output was truncated mid-JSON.
        if (action["exit_code"].as_i64().is_some_and(|code| code != 0)
            || action["status"] == "failed")
            && !state.failures.iter().any(|old| old["id"] == action["id"])
        {
            state.failures.push(action.clone());
        }
    }
    if state.failures.len() > 8 {
        state.failures.drain(..state.failures.len() - 8);
    }
}
