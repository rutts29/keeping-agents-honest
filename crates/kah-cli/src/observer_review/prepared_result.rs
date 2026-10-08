use super::{preparation, state::Store, workspace};
use crate::Result;
use kah_observer::{intake::Decision, session};
use serde_json::{Value, json};
use std::path::PathBuf;
mod packets;

pub fn consume(worker: &str) -> Result<()> {
    let store = Store::open_wait(worker)?;
    let mut state = store
        .load()?
        .ok_or("PREPARATION_STATE stage=context_preparation")?;
    let pending = state
        .pending
        .clone()
        .ok_or("PREPARATION_PENDING stage=context_preparation")?;
    let request = workspace::request(&store, &pending.id)?;
    if preparation::batch::packets::packets(&request).is_some() {
        drop(store);
        return packets::consume(worker);
    }
    preparation::accept(worker, &pending.id)?;
    let packet = &request.trigger["preparation"];
    let questions = serde_json::from_value(packet["questions"].clone())?;
    let home = PathBuf::from(
        std::env::var_os("HOME").ok_or("PREPARATION_HOME stage=context_preparation")?,
    );
    let evaluated = preparation::evaluate(&home, &request, &packet["state"], &questions);
    if evaluated
        .as_ref()
        .is_err_and(|e| e.to_string().starts_with("JEV_PREPARATION_PENDING"))
    {
        drop(store);
        return super::runtime::prepare_context(worker);
    }
    if evaluated
        .as_ref()
        .is_err_and(|e| e.to_string().starts_with("PREPARATION_CONTEXT_UNCONFIRMED"))
    {
        let error = evaluated.as_ref().err().unwrap().to_string();
        super::preparation::rejection::park(
            &store,
            &mut state,
            &request,
            &json!({"request":request.id,"purpose":"preparation","jev_state":{"unconfirmed_context":error},
                "assessment":{"passed":null,"violations":[]}}),
        )?;
        return finish(store, worker, &request.id);
    }
    let (judgment, input) = evaluated?;
    apply(&request, packet, &input, &judgment)?;
    state.latest = Some(
        json!({"request":pending.id,"purpose":"preparation","judgment":judgment,"prepared_state":input}),
    );
    state.pending = None;
    store.save(&state)?;
    finish(store, worker, &request.id)
}
fn finish(store: Store, worker: &str, request: &str) -> Result<()> {
    super::redelivery::completed(&store, request)?;
    for name in ["gap.json", "validation-current.json"] {
        let path = store.directory.join(name);
        if super::state::read::<Value>(&path)?.is_some_and(|v| {
            v["request"] == request
                && v["error"]
                    .as_str()
                    .is_some_and(|e| e.starts_with("PREPARATION_CONTEXT_UNCONFIRMED"))
        }) {
            std::fs::remove_file(path)?;
        }
    }
    super::timing::now(worker, request, "completed")?;
    drop(store);
    super::queue::kick(worker)
}
fn apply_steering(
    primary: &mut session::Store,
    input: &Value,
    judgment: &kah_observer::judge::Judgment,
) -> Result<()> {
    let text = input["event"]["user_text"]
        .as_str()
        .ok_or("PREPARATION_USER_TEXT stage=context_preparation")?;
    let source=primary.sources()?.into_iter().find(|s|s.text==text&&matches!(&s.decision,Some(Decision::Gap{reason}) if reason.starts_with("JEV_PREPARATION_PENDING")))
        .ok_or("PREPARATION_STEERING_SOURCE stage=context_preparation")?;
    primary.source_decision(
        source.key(),
        &Decision::Jev {
            judgment: Box::new(judgment.clone()),
        },
    )?;
    Ok(())
}
fn drift(
    request: &super::state::Request,
    input: &Value,
    judgment: &kah_observer::judge::Judgment,
) -> Result<()> {
    let actions = input["event"]["actions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if let Some(trigger) = kah_observer::verification::trigger(judgment, &actions) {
        super::queue::enqueue(
            &request.worker,
            super::state::Purpose::Drift,
            &request.turn,
            trigger,
        )?;
    }
    Ok(())
}

fn apply(
    request: &super::state::Request,
    packet: &Value,
    input: &Value,
    judgment: &kah_observer::judge::Judgment,
) -> Result<()> {
    let worker = &request.worker;
    let base = kah_core::run_store::RunStore::default_state_root()?;
    let mut primary = session::Store::open_wait(&base, worker)?;
    let mut source = primary
        .load()?
        .ok_or("PREPARATION_PRIMARY_STATE stage=context_preparation")?;
    if source.prepared_decisions.insert(request.id.clone()) {
        if packet["purpose"] == "steering" {
            apply_steering(&mut primary, input, judgment)?;
        }
        primary.append(&json!({"kind":"judgment","source":"observer","decision":request.id,"purpose":packet["purpose"],"question_set":kah_observer::judge::questions::VERSION,"judgment":judgment,"prepared":true,"at_ms":session::now_ms()}))?;
        source.judgments.push(judgment.clone());
        source.usage.calls += judgment.metrics.calls;
        source.usage.input_tokens += judgment.metrics.input_tokens;
        source.usage.cached_input_tokens += judgment.metrics.cached_input_tokens;
        source.usage.output_tokens += judgment.metrics.output_tokens;
        source.usage.elapsed_ms += judgment.metrics.elapsed_ms;
        source.usage.incomplete_calls += judgment.metrics.failed_calls;
    }
    source.gap = None;
    primary.save(&source)?;
    drop(primary);
    if packet["purpose"] == "drift" {
        drift(request, input, judgment)?;
    }
    Ok(())
}
