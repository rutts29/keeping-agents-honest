use super::*;
use crate::{
    intake::{self, Decision, Input},
    judge,
};
use kah_core::intent_history::{GoalRevision, History};

pub(super) fn update_intent(
    state: &mut State,
    journal: &mut Store,
    users: &NeutralSessionBundle,
) -> Result<(Option<ActiveProjection>, bool), String> {
    // Provider-owned full native sessions compact their conversations. kah
    // keeps canonical intent verbatim, including in Jev-only Untrusted flows.
    state.compaction = None;
    let source = InspectSource::from_bundle(users).map_err(|e| e.to_string())?;
    let previous = source
        .open(&state.binding.project)
        .map_err(|e| e.to_string())?
        .map(|r| r.recovered);
    state.run_id = previous.as_ref().map(|_| source.run_id().to_string());
    let mut history = previous.as_ref().map(|p| p.history.clone());
    let before = history.as_ref().map_or(0, |h| h.records().len());
    let mut inputs = journal.sources()?;
    let inherited = journal.inherited()?;
    validate_sources(&inputs, users)?;
    for (index, record) in users.records().iter().enumerate() {
        if index == inputs.len() {
            let quote = inherited.as_ref().and_then(|inbox| inbox.find(record.id()));
            let input = Input {
                source: quote
                    .map(|q| q.source.clone())
                    .unwrap_or_else(|| users.source_refs()[index].clone()),
                local_record: quote.map(|_| record.id().to_owned()),
                text: record.content().into(),
                decision: quote.and_then(crate::inherited::worker_item).map(|item| {
                    Decision::Inherited {
                        item: Box::new(item),
                    }
                }),
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
        let mut decision = intake::decide(&input.text, input.source.role.clone(), || {
            type_message(state, journal, history.as_ref(), &input.text)
        })
        .unwrap_or_else(|reason| Decision::Gap { reason });
        if crate::verification::enabled(&state.binding.session)?
            && let Decision::Jev { judgment } = &decision
            && crate::verification::uncertain_impact(judgment)
        {
            decision = Decision::Verification {
                judgment: judgment.clone(),
            };
        }
        journal.source_decision(input.key(), &decision)?;
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
            i.key() == r.id()
                && (i.local_record.is_some() || i.source.content_digest == r.content_digest())
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
