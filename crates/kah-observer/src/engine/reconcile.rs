use super::*;
use crate::{
    compaction,
    intake::{self, Decision, Input},
    judge,
};
use kah_core::intent_history::{GoalRevision, History};

pub(super) fn update_intent(
    state: &mut State,
    journal: &mut Store,
    users: &NeutralSessionBundle,
    config: CompactorConfig<'_>,
) -> Result<(Option<ActiveProjection>, bool), String> {
    let source = InspectSource::from_bundle(users).map_err(|e| e.to_string())?;
    let previous = source
        .open(&state.binding.project)
        .map_err(|e| e.to_string())?
        .map(|r| r.recovered);
    state.run_id = previous.as_ref().map(|_| source.run_id().to_string());
    let mut history = previous.as_ref().map(|p| p.history.clone());
    let before = history.as_ref().map_or(0, |h| h.records().len());
    let mut inputs = journal.sources()?;
    validate_sources(&inputs, users)?;
    for (index, record) in users.records().iter().enumerate() {
        if index == inputs.len() {
            let input = Input {
                source: users.source_refs()[index].clone(),
                text: record.content().into(),
                decision: None,
                dismissed: false,
                promoted: false,
            };
            journal.add_source(&input)?;
            inputs.push(input);
        }
        resolve_input(state, journal, &mut history, &mut inputs[index])?;
    }
    let mut projection = history
        .as_ref()
        .map(|h| {
            ActiveProjection::from_history(h, ProjectionConfig::default())
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    if let (Some(h), Some(p)) = (&history, &mut projection) {
        p.generated_by.component = "kah.user-controlled-verbatim".into();
        persist(&state.binding.project, &source, previous.as_ref(), h, p)?;
        state.run_id = Some(source.run_id().to_string());
        compact(state, journal, p, config)?;
    }
    if let Some(input) = inputs.iter().find(|i| i.status(history.as_ref()) == "gap") {
        return Err(match &input.decision {
            Some(Decision::Gap { reason }) => reason.clone(),
            _ => "OBSERVER_SOURCE_PENDING stage=intake".into(),
        });
    }
    Ok((
        projection,
        history.as_ref().is_some_and(|h| h.records().len() > before),
    ))
}
fn resolve_input(
    state: &mut State,
    journal: &mut Store,
    history: &mut Option<History>,
    input: &mut Input,
) -> Result<(), String> {
    if input.decision.is_none() {
        let decision = intake::decide(&input.text, input.source.role.clone(), || {
            type_message(state, journal, history.as_ref(), &input.text)
        })
        .unwrap_or_else(|reason| Decision::Gap { reason });
        journal.source_decision(&input.source.record, &decision)?;
        input.decision = Some(decision);
    }
    let decision = input.decision.as_ref().unwrap();
    if decision.records_intent() && !input.dismissed {
        let h = history.get_or_insert(
            History::new(GoalRevision::new(
                1,
                &input.text,
                vec![input.source.clone()],
            ))
            .map_err(|e| e.to_string())?,
        );
        decision.apply(h, &input.text, input.source.clone())?;
    }

    Ok(())
}

fn validate_sources(inputs: &[Input], users: &NeutralSessionBundle) -> Result<(), String> {
    if users.records().iter().any(|r| !r.is_authoritative()) {
        return Err("OBSERVER_AUTHORITY stage=intake".into());
    }
    if inputs.len() > users.records().len()
        || !inputs.iter().zip(users.records()).all(|(i, r)| {
            i.source.record == r.id()
                && i.source.content_digest == r.content_digest()
                && i.text == r.content()
                && r.is_authoritative()
        })
    {
        return Err("OBSERVER_SOURCE_REWRITE stage=intake".into());
    }
    Ok(())
}
fn type_message(
    state: &mut State,
    journal: &mut Store,
    history: Option<&History>,
    text: &str,
) -> Result<judge::Judgment, String> {
    let items = history
        .map(|h| {
            h.intents()
                .values()
                .filter(|i| i.is_active())
                .map(|i| (i.id.to_string(), i.text.clone()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let questions = judge::questions::steering(&items);
    let goal = history.map(|h| h.goal().text.as_str());
    let goal_id = history
        .and_then(|h| {
            h.intents().values().find(|i| {
                i.introduced_by
                    .iter()
                    .any(|r| h.goal().source_refs.contains(r))
            })
        })
        .map(|i| i.id.as_str());
    let input = judge::questions::steering_state(goal, goal_id, &items, text, "");
    super::judgment::evaluate(state, journal, &input, &questions, "steering")
}
fn compact(
    state: &mut State,
    journal: &mut Store,
    projection: &mut ActiveProjection,
    config: CompactorConfig<'_>,
) -> Result<(), String> {
    if projection.is_overflowed()
        && state
            .compaction
            .as_ref()
            .is_none_or(|c| c.source_digest != compaction::digest(projection))
    {
        let (summary, metadata) = compaction::compact(&state.binding.project, projection, config)
            .inspect_err(|_| state.usage.incomplete_calls += 1)?;
        state.usage.record(&metadata);
        journal.append(&json!({"kind":"compaction","summary":summary,"metadata":metadata}))?;
        state.compaction = Some(summary);
    }
    compaction::attach(projection, state.compaction.as_ref());
    Ok(())
}
pub(crate) fn persist(
    project: &std::path::Path,
    source: &InspectSource,
    previous: Option<&kah_core::run_store::RecoveredRun>,
    history: &History,
    projection: &ActiveProjection,
) -> Result<(), String> {
    if let Some(previous) = previous {
        let store =
            RunStore::open_default(project, source.run_id().as_str()).map_err(|e| e.to_string())?;
        for record in &history.records()[previous.history.records().len()..] {
            store
                .append_pending(record.clone())
                .map_err(|e| e.to_string())?;
        }
        store
            .commit_projection_with_source_cursor(projection, source.cursor())
            .map_err(|e| e.to_string())?;
    } else {
        source
            .create(project, history, projection)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
