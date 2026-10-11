//! Current-rule rebuilds are intent jobs on the SAME persistent native observer.
use super::super::state::{Purpose, State, Store};
use super::{authority, native};
use crate::Result;
use kah_core::{
    current_rules::{Input, RuleSet},
    run_store::{RuleRebuild, RuleSync, RunStore},
};
use kah_observer::{
    current_rules,
    rule_source::{CurrentSet, Resolved, Source},
    session,
};
use serde_json::{Value, json};
use std::path::Path;
pub fn binding(worker: &str) -> Result<session::Binding> {
    Ok(
        session::Store::read(&RunStore::default_state_root()?, worker)?
            .ok_or("RULE_SET_SESSION stage=current_rules")?
            .binding,
    )
}
pub fn trigger(rebuild: &RuleRebuild) -> Result<Value> {
    Ok(
        json!({"kind":"current_rules_rebuild","input_digest":rebuild.input.digest()?,"rebuild":rebuild}),
    )
}
pub fn enqueue(binding: &session::Binding, turn: &str) -> Result<bool> {
    if let Some(RuleSync::Rebuild(rebuild)) = current_rules::stage(binding)? {
        super::super::queue::enqueue(
            &binding.session,
            Purpose::Steering,
            turn,
            trigger(&rebuild)?,
        )?;
        return Ok(true);
    }
    Ok(false)
}
pub fn submit(
    store: Store,
    state: State,
    home: &Path,
    turn: &str,
    original: Value,
) -> Result<Value> {
    let b = binding(&state.worker.session)?;
    let Some(RuleSync::Rebuild(rebuild)) = current_rules::stage(&b)? else {
        return Ok(json!({"status":"option1_reviewed","rules":"already_current"}));
    };
    let head = format!("intent-{}", rebuild.input.digest()?);
    let history = kah_core::intent_history::History::replay(rebuild.input.history.clone())?;
    let p =
        kah_core::projection::project(&history, kah_core::projection::ProjectionConfig::default())?;
    let intent = json!({"revision":p.revision,"goal":p.goal.text,"goal_source":p.goal.source_refs,"items":p.preserve.iter().chain(&p.frontier).chain(&p.validation).map(|i|json!({"id":i.id,"text":i.text,"source":i.introduced_by})).collect::<Vec<_>>()});
    native::submit(
        store,
        state,
        home,
        turn,
        &head,
        json!({"stage":"current_rules","input_digest":rebuild.input.digest()?,"intent":intent,"rebuild":rebuild}),
        original,
    )
}
pub fn resolve(worker: &str, observer: &str, head: &str) -> Result<Option<Resolved>> {
    let b = binding(worker)?;
    if !settled(&b)? {
        return Ok(None);
    }
    let current_rules::CommitRoute::JevRules { set_digest, .. } =
        current_rules::for_commit(&b, head)?
    else {
        return Ok(None);
    };
    let Some(set) = current_rules::current(&b)? else {
        return Ok(None);
    };
    let receipt: Option<Value> =
        super::super::state::read(&Store::directory(worker)?.join("current-rules-observer.json"))?;
    if !receipt.is_some_and(|r| r["observer_session"] == observer && r["set_digest"] == set_digest)
    {
        return Ok(None);
    }
    let input = Input::from_history(
        &authority::history(worker)?,
        current_rules::standing_sources(&b.project)?,
        "now",
    )?;
    if set.digest()? != set_digest || set.as_set().input_digest != input.digest()? {
        return Ok(None);
    }
    Ok(Some(CurrentSet::new(&set, &input, observer)?.load(&[])?))
}
fn settled(binding: &session::Binding) -> Result<bool> {
    let root = RunStore::default_state_root()?;
    if session::Store::read(&root, &binding.session)?.is_some_and(|s| s.gap.is_some()) {
        return Ok(false);
    }
    let store = session::Store::open_wait(&root, &binding.session)?;
    Ok(!store.sources()?.iter().any(|i| {
        !i.dismissed
            && !i.promoted
            && matches!(
                i.decision,
                None | Some(
                    kah_observer::intake::Decision::Verification { .. }
                        | kah_observer::intake::Decision::Gap { .. }
                )
            )
    }))
}
pub fn accept(worker: &str, observer: &str, candidate: &RuleSet) -> Result<Value> {
    if observer.is_empty() {
        return Err("RULE_SET_OBSERVER stage=current_rules".into());
    }
    let b = binding(worker)?;
    // A crash after immutable adoption but before the controller receipt must
    // replay the same validated ACK without reopening native inference.
    let already = current_rules::current(&b)?.filter(|s| s.as_set() == candidate);
    let adopted = match already {
        Some(set) => Ok(set),
        None => current_rules::accept(&b, candidate),
    };
    match adopted {
        Ok(set) => {
            let result = json!({"status":"validated","set_digest":set.digest()?,"observer_session":observer,"intent_revision":set.revision(),"conflicts":set.has_conflicts()});
            crate::unattended_report::save(
                &Store::directory(worker)?,
                "current-rules-observer.json",
                &result,
            )?;
            Ok(result)
        }
        Err(e) if e.starts_with("RULE_SET_BINDING") => {
            enqueue(&b, "intent-changed-during-rebuild")?;
            Ok(json!({"status":"stale_candidate_retained","reason":e}))
        }
        Err(e) => Err(e.into()),
    }
}
