//! Event-driven observation. Native hooks schedule work; only native user
//! records can change intent, and observer failures remain visible but fail open.
use crate::{
    inspect_state::InspectSource,
    session::{Binding, State, Store},
};
use kah_codex::{
    control::Control,
    overlay::{self, Policy},
    source,
};
use kah_core::{
    observation::{ContextInjection, ObservationSnapshot},
    projection::{ActiveProjection, ProjectionConfig},
    run_store::RunStore,
    session_bundle::NeutralSessionBundle,
};
use serde_json::json;
use std::{env, path::PathBuf, time::Duration};

mod context;
mod judgment;
pub(crate) mod reconcile;
use reconcile::*;

#[derive(Default)]
pub struct Outcome {
    pub verification: Option<serde_json::Value>,
    pub projection: Option<ActiveProjection>,
    pub reminder: Option<ContextInjection>,
    pub gap: Option<String>,
}

/// Synchronize whole user history without assessing ordinary tool/turn actions.
pub fn synchronize(binding: &Binding, turn: &str) -> Result<Outcome, String> {
    observe_with(binding, turn, || read_source(binding))
}

pub fn synchronize_snapshot(
    binding: &Binding,
    turn: &str,
    snapshot: ObservationSnapshot,
) -> Result<Outcome, String> {
    validate_snapshot(binding, &snapshot)?;
    observe_with(binding, turn, || Ok(snapshot))
}

fn observe_with(
    binding: &Binding,
    turn: &str,
    source: impl FnOnce() -> Result<ObservationSnapshot, String>,
) -> Result<Outcome, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    if crate::session::paused(&base, &binding.session) {
        return Ok(Outcome::default());
    }
    let mut store = Store::open(&base, &binding.session, Duration::from_secs(110))?;
    let mut state = store.load()?.unwrap_or_else(|| State::new(binding.clone()));
    if state.binding.project != binding.project {
        return Err("OBSERVER_BINDING stage=observe: session project changed".into());
    }
    // The next success clears the gap, so journal it first, as the hook does.
    if state.recover_interrupted() {
        store.append(&json!({"kind":"gap","source":"observer","gap":state.gap}))?;
    }
    state.binding = binding.clone();
    state.begin(turn);
    store.save(&state)?;
    let result = source().and_then(|snapshot| update(&mut state, &mut store, snapshot));
    match result {
        Ok(outcome) => {
            state.inflight = None;
            state.gap = None;
            state.observations += 1;
            store.save(&state)?;
            Ok(outcome)
        }
        Err(error) if error.starts_with("JEV_PREPARATION_PENDING") => {
            let packet = context::pending(&state)?;
            state.inflight = None;
            state.gap = None;
            store.save(&state)?;
            Ok(Outcome {
                verification: Some(json!({"preparation":packet})),
                projection: projection(&state)?,
                reminder: None,
                gap: None,
            })
        }
        Err(error) => {
            let code = error
                .split_whitespace()
                .next()
                .unwrap_or("OBSERVER_UNKNOWN")
                .to_owned();
            state.fail(&code, error.clone());
            store.save(&state)?;
            store.append(&json!({"kind":"gap","source":"observer","detail":error,"at_ms":crate::session::now_ms()}))?;
            Ok(Outcome {
                verification: None,
                projection: projection(&state)?,
                reminder: None,
                gap: Some(error),
            })
        }
    }
}

fn update(
    state: &mut State,
    store: &mut Store,
    mut snapshot: ObservationSnapshot,
) -> Result<Outcome, String> {
    if let Some(native) = snapshot
        .native_candidates
        .as_ref()
        .or(snapshot.users.as_ref())
    {
        snapshot.users = store.classified_users(native)?;
    }
    snapshot.users = crate::inherited::merge(&state.binding, store, snapshot.users)?;
    let Some(users) = &snapshot.users else {
        return Ok(Outcome::default());
    };
    let (projection, _) = update_intent(state, store, users)?;
    Ok(Outcome {
        projection,
        ..Outcome::default()
    })
}

pub fn projection(state: &State) -> Result<Option<ActiveProjection>, String> {
    let Some(id) = &state.run_id else {
        return Ok(None);
    };
    let store = RunStore::open_default(&state.binding.project, id).map_err(|e| e.to_string())?;
    let projection = store.recover().map_err(|e| e.to_string())?.projection;
    Ok(Some(projection))
}

/// Only detached review preparation may wait on the intent writer. Tool hooks
/// keep the nonblocking projection above and their existing queue semantics.
pub fn projection_wait(state: &State) -> Result<Option<ActiveProjection>, String> {
    let Some(id) = &state.run_id else {
        return Ok(None);
    };
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let store =
        RunStore::open_wait(&base, &state.binding.project, id).map_err(|e| e.to_string())?;
    Ok(Some(store.recover().map_err(|e| e.to_string())?.projection))
}

fn read_source(binding: &Binding) -> Result<ObservationSnapshot, String> {
    let mut client = native_control(binding)?;
    source::observe(&mut client, &binding.project, &binding.session).map_err(|e| e.to_string())
}

/// The restricted native control client every Codex intake reads through.
pub fn native_control(binding: &Binding) -> Result<Control, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("OBSERVER_HOME stage=observe: HOME is absent")?;
    let directory = RunStore::default_state_root()
        .map_err(|e| e.to_string())?
        .join("codex/control");
    overlay::prepare(
        &binding.codex_home,
        &directory,
        &Policy {
            restricted: true,
            online: false,
            root: &binding.project,
            cwd: &binding.project,
        },
    )
    .map_err(|e| e.to_string())?;
    let native = if binding.native.is_absolute() {
        binding.native.clone()
    } else {
        home.join(".local/bin/codex")
    };
    Control::connect(
        &native,
        &directory,
        &binding.project,
        Duration::from_secs(10),
    )
    .map_err(|e| e.to_string())
}

fn validate_snapshot(binding: &Binding, snapshot: &ObservationSnapshot) -> Result<(), String> {
    use sha2::{Digest, Sha256};
    for users in snapshot
        .users
        .iter()
        .chain(snapshot.native_candidates.iter())
    {
        if users.source().session_ref_private() != binding.session {
            return Err(
                "OBSERVER_SOURCE_SESSION stage=worker_intake: source belongs to another session"
                    .into(),
            );
        }
        let project = binding
            .project
            .canonicalize()
            .map_err(|e| format!("OBSERVER_SOURCE_PROJECT stage=worker_intake: {e}"))?;
        let digest = format!(
            "sha256:{:x}",
            Sha256::digest(project.as_os_str().as_encoded_bytes())
        );
        if users.source().repository_digest() != digest {
            return Err(
                "OBSERVER_SOURCE_PROJECT stage=worker_intake: source belongs to another project"
                    .into(),
            );
        }
    }
    let bytes = serde_json::to_vec(&snapshot.actions).map_err(|e| e.to_string())?;
    let digest = format!("sha256:{:x}", Sha256::digest(bytes));
    if snapshot.action_digest != digest {
        return Err("OBSERVER_ACTION_DIGEST stage=worker_intake".into());
    }
    Ok(())
}
