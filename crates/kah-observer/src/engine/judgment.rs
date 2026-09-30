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
) -> Result<judge::decision::Assessment, String> {
    let prior_items = context.prior.map(items).unwrap_or_default();
    let questions = judge::questions::drift();
    let mut input = bounded_state(state, &context, &prior_items);
    let ignored = store.sources()?.into_iter().rev().filter(crate::intake::Input::recently_ignored).take(4).map(|i|json!({"source_record":i.source.record,"text":bounded(&i.text,1200),"classification":"previously ignored; user source, not accepted intent"})).collect::<Vec<_>>();
    input["intent"]["recent_ignored_user_messages"] = json!(ignored);
    let judgment = evaluate(state, store, &input, &questions, "drift")?;
    let assessment = judge::decision::assessment(&judgment, context.actions, state.failures.len());
    store.append(&json!({"kind":"assessment","source":"observer","assessment":assessment,"turn":context.turn}))?;
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
        .filter_map(|a| {
            a["evidence"]["text"]
                .as_str()
                .or_else(|| a["evidence"].as_str())
        })
        .collect::<Vec<_>>()
        .join("\n");
    json!({
        "prior_intent":{"goal":context.prior.map(|p|&p.goal.text),"items":prior_items},
        "intent":{"goal":context.projection.goal.text,"items":items(context.projection),"failures":state.failures,"completed_outcomes":[],"open_outcomes":[]},
        "event":{"user_text":bounded(user,4096),"agent_text":bounded(&agent,4096),"actions":bounded_actions(context.actions)}
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
    state.usage.calls += judgment.metrics.calls;
    state.usage.input_tokens += judgment.metrics.input_tokens;
    state.usage.cached_input_tokens += judgment.metrics.cached_input_tokens;
    state.usage.output_tokens += judgment.metrics.output_tokens;
    state.usage.elapsed_ms += judgment.metrics.elapsed_ms;
    state.usage.incomplete_calls += judgment.metrics.failed_calls;
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
    let judgment = judge::routing::evaluate(
        &home,
        state.binding.judge,
        &state.binding.jev_model,
        input,
        questions,
    )
    .map_err(|e| {
        state.usage.incomplete_calls += 1;
        e.to_string()
    })?;
    record_usage(state, &judgment);
    let digest = format!("sha256:{:x}", Sha256::digest(input.to_string().as_bytes()));
    store.append(&json!({"kind":"judgment","source":"observer","purpose":purpose,"question_set":judge::questions::VERSION,"state_digest":digest,"judgment":judgment,"at_ms":crate::session::now_ms()}))?;
    state.judgments.push(judgment.clone());
    if state.judgments.len() > 16 {
        state.judgments.remove(0);
    }
    Ok(judgment)
}

fn bounded_actions(actions: &[Value]) -> Vec<Value> {
    let mut bytes = 0;
    let mut selected = actions
        .iter()
        .rev()
        .take_while(|action| {
            bytes += action.to_string().len();
            bytes <= 24 * 1024
        })
        .cloned()
        .collect::<Vec<_>>();
    selected.reverse();
    selected
}
