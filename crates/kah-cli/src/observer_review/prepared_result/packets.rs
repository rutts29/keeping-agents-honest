use super::*;
use crate::observer_review::{preparation::batch, state::Request};
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
    let active = preparation::active_decision(worker, &request.id)?;
    if !preparation::accept(worker, &active)? {
        drop(store);
        return crate::observer_review::runtime::prepare_context(worker);
    }
    let _processing = crate::observer_review::liveness::processing(worker, &request.id)?;
    let packets =
        batch::packets::packets(&request).ok_or("PREPARATION_PACKETS stage=context_preparation")?;
    let home = PathBuf::from(
        std::env::var_os("HOME").ok_or("PREPARATION_HOME stage=context_preparation")?,
    );
    let mut results = vec![];
    for (index, packet) in packets.iter().enumerate() {
        let job = batch::packets::job_for_packet(&request, index, packet)?;
        let result = one(&home, &request, packet, &job);
        match result {
            Ok(receipt) => results.push(receipt),
            Err(error) if error.to_string().starts_with("JEV_PREPARATION_PENDING") => {
                drop(store);
                return crate::observer_review::runtime::prepare_context(worker);
            }
            Err(error) => return Err(error),
        }
    }
    state.latest = Some(
        json!({"request":request.id,"purpose":"preparation_batch","results":results,"packets":packets.len()}),
    );
    state.pending = None;
    store.save(&state)?;
    super::finish(store, worker, &request.id)
}
fn one(
    home: &std::path::Path,
    request: &Request,
    packet: &Value,
    job: &preparation::Job,
) -> Result<Value> {
    let root = Store::directory(&request.worker)?;
    let name = format!("packet-result-{}.json", job.decision);
    let identity = kah_observer::context_selection::digest(
        &json!({"original":job.state,"questions":job.questions,"packet":packet}),
    );
    if let Some(saved) = crate::observer_review::state::read::<Value>(&root.join(&name))? {
        if saved["original_digest"] != identity {
            return Err("PREPARATION_PACKET_CHANGED stage=context_preparation".into());
        }
        return Ok(saved);
    }
    let mut local = request.clone();
    local.id = job.decision.clone();
    local.turn = packet["turn"]
        .as_str()
        .ok_or("PREPARATION_PACKET_TURN stage=context_preparation")?
        .into();
    local.intent = job.state["intent"].clone();
    let (judgment, input) = match preparation::evaluate(home, &local, &job.state, &job.questions) {
        Ok(result) => result,
        Err(error)
            if error
                .to_string()
                .starts_with("PREPARATION_CONTEXT_UNCONFIRMED") =>
        {
            return terminal(
                &root,
                &name,
                &identity,
                &local,
                request,
                packet,
                error.as_ref(),
            );
        }
        Err(error) => return Err(error),
    };
    let original = batch::packet_refs::original(packet)?;
    apply(
        &local,
        &original["trigger"]["preparation"],
        &input,
        &judgment,
    )?;
    let receipt = json!({"decision":job.decision,"original_digest":identity,"turn":local.turn,"original_queue_source":packet["source"],"judgment":judgment,"prepared_state":input});
    crate::observer_review::source_store::save(&root, &name, &receipt)?;
    Ok(receipt)
}
fn terminal(
    root: &std::path::Path,
    name: &str,
    identity: &str,
    local: &Request,
    parent: &Request,
    packet: &Value,
    error: &dyn std::error::Error,
) -> Result<Value> {
    let receipt = json!({"decision":local.id,"original_digest":identity,"turn":local.turn,
        "original_queue_source":packet["source"],"terminal":"unconfirmed_context","passed":null,
        "error":error.to_string(),"source":"context-rejections","no_automatic_reasks":true});
    crate::observer_review::source_store::save(root, name, &receipt)?;
    crate::observer_review::reminder_queue::save(
        root,
        &json!({"request":local.id,"purpose":"uncertainty","turn":local.turn,"system_notice":true,
        "text":format!("kah observer retained unconfirmed context for {} (review {}): {error}. No Jev decision/pass was fabricated; original state and selection remain archived. Other packets continue.",local.id,parent.id)}),
    )?;
    Ok(receipt)
}
