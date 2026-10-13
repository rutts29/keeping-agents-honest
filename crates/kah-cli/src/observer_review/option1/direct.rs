//! Missing/currently unsettled rules never become a stale or empty Jev pass.
use super::super::state::{State, Store};
use super::{authority, capture, current_set, input, journal, native};
use crate::Result;
use kah_observer::{
    commit_review,
    rule_source::{Link, Resolved, Rule},
};
use serde_json::{Value, json};
use std::path::Path;
pub fn submit(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    head: &str,
    trigger: Value,
) -> Result<Value> {
    let b = current_set::binding(&state.worker.session)?;
    current_set::enqueue(&b, turn)?;
    let history = match authority::history(&b.session) {
        Ok(h) => Some(h),
        Err(e) if e.to_string().starts_with("COMMIT_NO_INTENT") => None,
        Err(e) => return Err(e),
    };
    let standing = kah_observer::current_rules::standing_sources(&b.project)?;
    let checkpoint = trigger["review_class"] != "commit";
    let mut context = context_for(home, &state, &store.directory, head, checkpoint)?;
    super::binary::publish(&store, &state, &mut context)?;
    let (input, sources) = source_input(
        history.as_ref(),
        standing,
        &context,
        state
            .observer
            .session
            .as_deref()
            .unwrap_or("pending-native-binding"),
    )?;
    let mut record = commit_review::classify(&b.session, head, &[], &journal::policy(home)?)?;
    record.gaps.push(json!({"route":"direct_observer_review","reason":"current_rule_set_unavailable","full_sources_retained":true}));
    journal::save(&record)?;
    let primary = kah_observer::session::Store::open_wait(
        &kah_core::run_store::RunStore::default_state_root()?,
        &b.session,
    )?;
    let pending = primary.sources()?;
    drop(primary);
    let plan = if checkpoint {
        let files = input
            .files
            .iter()
            .map(|(p, f)| (p.clone(), f.digest.clone()))
            .collect();
        json!(commit_review::checkpoint::plan(
            &journal::all(&b.session)?,
            &commit_review::authority(&input),
            &files
        ))
    } else {
        Value::Null
    };
    native::submit(
        store,
        state,
        home,
        turn,
        head,
        json!({"stage":if checkpoint{"checkpoint"}else{"confirmation"},"input":input,"record":record,"plan":plan,"source_catalogue":sources,"pending_intake":pending,"direct_reason":"Resolve current applicability from full original sources. No validated set exists or intake is unsettled; do not assume pending words are absent. Unresolved applicability remains unclear.","original_trigger":trigger}),
        trigger.clone(),
    )
}
pub fn context(home: &Path, state: &State, directory: &Path, head: &str) -> Result<Value> {
    context_for(home, state, directory, head, false)
}
/// A checkpoint judges the cumulative change from the checkpoint baseline, as
/// the validated-set admission does; a commit review judges one commit.
pub fn context_for(
    home: &Path,
    state: &State,
    directory: &Path,
    head: &str,
    checkpoint: bool,
) -> Result<Value> {
    let name = if checkpoint {
        "option1-checkpoint-baseline.json"
    } else {
        "option1-baseline.json"
    };
    let baseline: Option<Value> = super::super::state::read(&directory.join(name))?;
    let base = baseline.as_ref().and_then(|v| v["head"].as_str());
    capture::project(home, state, base, head)
}
fn source_input(
    history: Option<&kah_core::intent_history::History>,
    standing: Vec<kah_core::current_rules::Source>,
    context: &Value,
    observer: &str,
) -> Result<(kah_observer::commit_check::Input, Value)> {
    if let Some(h) = history {
        let source = kah_core::current_rules::Input::from_history(h, standing, "now")?;
        let raw = fallback(&source, observer)?;
        Ok((input::build(h, &raw, context, true)?, json!(source)))
    } else {
        let raw = from_sources(
            &standing,
            observer,
            kah_observer::context_selection::digest(&json!(standing)),
            None,
        );
        Ok((
            input::without_history(&raw, context)?,
            json!({"accepted_history":null,"standing":standing}),
        ))
    }
}
pub fn fallback(input: &kah_core::current_rules::Input, observer: &str) -> Result<Resolved> {
    Ok(from_sources(
        &input.sources,
        observer,
        input.digest()?,
        Some(input.history_digest.clone()),
    ))
}
fn from_sources(
    sources: &[kah_core::current_rules::Source],
    observer: &str,
    digest: String,
    history_digest: Option<String>,
) -> Resolved {
    Resolved {
        source_digest:digest,observer_session:observer.into(),history_digest,
        source_words:sources.iter().map(|s|(s.id.clone(),s.text.clone())).collect(),
        rules:sources.iter().filter(|s|!s.text.trim().is_empty()).map(|s|Rule {
            id:format!("source:{}",s.id),predicate:s.text.clone(),scope:"Unvalidated source: determine applicability from its exact History lifecycle and standing scope; historical context is not a new current obligation".into(),
            diff_checkable:false,sources:vec![Link {source_id:s.id.clone(),quote:s.text.clone()}],permissions:vec![],kind:Default::default(),
        }).collect(),
    }
}
