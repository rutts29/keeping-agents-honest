//! Event-driven observation. Native hooks schedule work; only native user
//! records can change intent, and observer failures remain visible but fail open.
use crate::{
    inspect_state::InspectSource,
    judge::decision::{Assessment, AssessmentState},
    semantic_resolver::{self, CompactorConfig},
    session::{Binding, State, Store},
};
use kah_codex::{
    control::Control,
    overlay::{self, Policy},
    source::{self, ObservationSnapshot},
};
use kah_core::{
    observation::{ContextInjection, InjectionKind},
    projection::{ActiveProjection, ProjectionConfig},
    run_store::{RunStore, SourceCursorComparison},
    session_bundle::NeutralSessionBundle,
};
use serde_json::json;
use std::{env, path::PathBuf, time::Duration};

mod judgment;
mod reconcile;
use reconcile::*;

pub struct Outcome {
    pub projection: Option<ActiveProjection>,
    pub reminder: Option<ContextInjection>,
    pub gap: Option<String>,
}

pub fn observe(binding: &Binding, turn: &str, allow_reminder: bool) -> Result<Outcome, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    let mut store = Store::open(&base, &binding.session, Duration::from_secs(110))?;
    let mut state = store.load()?.unwrap_or_else(|| State::new(binding.clone()));
    if state.binding.project != binding.project {
        return Err("OBSERVER_BINDING stage=observe: session project changed".into());
    }
    state.recover_interrupted();
    state.binding = binding.clone();
    state.begin(turn);
    store.save(&state)?;
    let result = update(&mut state, &mut store, turn, allow_reminder);
    match result {
        Ok(outcome) => {
            state.inflight = None;
            state.gap = None;
            state.observations += 1;
            store.save(&state)?;
            Ok(outcome)
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
    turn: &str,
    allow_reminder: bool,
) -> Result<Outcome, String> {
    let binding = state.binding.clone();
    let snapshot = read_source(&binding)?;
    let Some(users) = &snapshot.users else {
        return Ok(Outcome {
            projection: None,
            reminder: None,
            gap: None,
        });
    };
    let config = CompactorConfig::new(&binding.model, Some(&binding.effort))?
        .with_timeout(Duration::from_secs(binding.timeout_secs))?;
    let prior = projection(state)?;
    let (projection, new_user) = update_intent(state, store, users, config)?;
    if new_user {
        state.drift_active = false;
    }
    if state.action_digest.as_deref() == Some(&snapshot.action_digest)
        || snapshot.actions.is_empty()
    {
        return Ok(Outcome {
            projection: Some(projection),
            reminder: None,
            gap: None,
        });
    }
    let assessment = judgment::assess(
        state,
        store,
        judgment::Context {
            prior: prior.as_ref(),
            projection: &projection,
            users,
            actions: &snapshot.actions,
            new_user,
            turn,
        },
        config,
    )?;
    state.action_digest = Some(snapshot.action_digest);
    let reminder = drift_reminder(state, &assessment, &projection, turn, allow_reminder)?;
    Ok(Outcome {
        projection: Some(projection),
        reminder,
        gap: None,
    })
}

pub fn projection(state: &State) -> Result<Option<ActiveProjection>, String> {
    let Some(id) = &state.run_id else {
        return Ok(None);
    };
    let store = RunStore::open_default(&state.binding.project, id).map_err(|e| e.to_string())?;
    Ok(Some(store.recover().map_err(|e| e.to_string())?.projection))
}

fn read_source(binding: &Binding) -> Result<ObservationSnapshot, String> {
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
    let mut client = Control::connect(
        &native,
        &directory,
        &binding.project,
        Duration::from_secs(10),
    )
    .map_err(|e| e.to_string())?;
    source::observe(&mut client, &binding.project, &binding.session).map_err(|e| e.to_string())
}

fn drift_reminder(
    state: &mut State,
    assessment: &Assessment,
    projection: &ActiveProjection,
    turn: &str,
    allow_reminder: bool,
) -> Result<Option<ContextInjection>, String> {
    let reminder = if assessment.state == AssessmentState::MaterialDrift {
        let should_remind =
            allow_reminder && !state.drift_active && state.reminded_turn.as_deref() != Some(turn);
        state.drift_active = true;
        if should_remind {
            state.reminded_turn = Some(turn.into());
            state.reminders += 1;
            Some(ContextInjection::new(
                projection.revision,
                InjectionKind::DriftReminder,
                format!(
                    "[kah observer reminder; not user steering]\n{}\nCheck this against the user's current intent and correct the material deviation. Current intent: {}",
                    assessment.reason,
                    projection.rendered_prompt()
                ),
            )?)
        } else {
            None
        }
    } else {
        if assessment.state == AssessmentState::Aligned {
            state.drift_active = false;
        }
        None
    };
    Ok(reminder)
}
