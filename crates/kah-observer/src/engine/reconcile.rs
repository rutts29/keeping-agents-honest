use super::*;
use crate::{compaction, judge, verbatim};
use kah_core::intent_history::{GoalRevision, History};

pub(super) fn update_intent(
    state: &mut State,
    journal: &mut Store,
    users: &NeutralSessionBundle,
    config: CompactorConfig<'_>,
) -> Result<(ActiveProjection, bool), String> {
    let source = InspectSource::from_bundle(users).map_err(|e| e.to_string())?;
    let previous = source
        .open(&state.binding.project)
        .map_err(|e| e.to_string())?
        .map(|r| r.recovered);
    state.run_id = previous.as_ref().map(|_| source.run_id().to_string());
    let accepted = match &previous {
        Some(run) => {
            let cursor = run
                .source_cursor
                .as_ref()
                .ok_or("OBSERVER_CURSOR stage=verbatim")?;
            match cursor.compare(source.cursor()).map_err(|e| e.to_string())? {
                SourceCursorComparison::Current | SourceCursorComparison::NewRecords(_) => {
                    cursor.accepted_record_count()
                }
                _ => return Err("OBSERVER_SOURCE_REWRITE stage=verbatim".into()),
            }
        }
        None => 0,
    };
    let mut result = previous.map(|p| p.projection);
    for end in accepted + 1..=users.records().len() {
        let prefix = NeutralSessionBundle::from_records_with_native_goal(
            users.source().provider(),
            users.source().session_ref_private(),
            users.source().repository_digest(),
            users.native_goal().clone(),
            users.records()[..end].to_vec(),
        )
        .map_err(|e| e.to_string())?;
        result = Some(update_once(state, journal, &prefix)?);
        journal.save(state)?;
    }
    let mut projection = result.ok_or("OBSERVER_SOURCE_EMPTY stage=verbatim")?;
    if projection.is_overflowed()
        && state
            .compaction
            .as_ref()
            .is_none_or(|c| c.source_digest != compaction::digest(&projection))
    {
        let (summary, metadata) = compaction::compact(&state.binding.project, &projection, config)
            .inspect_err(|_| state.usage.incomplete_calls += 1)?;
        state.usage.record(&metadata);
        journal.append(&json!({"kind":"compaction","summary":summary,"metadata":metadata}))?;
        state.compaction = Some(summary);
    }
    compaction::attach(&mut projection, state.compaction.as_ref());
    Ok((projection, accepted < users.records().len()))
}

fn update_once(
    state: &mut State,
    journal: &mut Store,
    users: &NeutralSessionBundle,
) -> Result<ActiveProjection, String> {
    let source = InspectSource::from_bundle(users).map_err(|e| e.to_string())?;
    let previous = source
        .open(&state.binding.project)
        .map_err(|e| e.to_string())?
        .map(|r| r.recovered);
    let record = users
        .records()
        .last()
        .ok_or("OBSERVER_SOURCE_EMPTY stage=verbatim")?;
    if !record.is_authoritative() {
        return Err("OBSERVER_AUTHORITY stage=verbatim".into());
    }
    let reference = users
        .source_refs()
        .pop()
        .ok_or("OBSERVER_SOURCE_EMPTY stage=verbatim")?;
    let mut history = match &previous {
        Some(run) => run.history.clone(),
        None => History::new(GoalRevision::new(
            1,
            record.content(),
            vec![reference.clone()],
        ))
        .map_err(|e| e.to_string())?,
    };
    let cached = journal.intake(&reference)?;
    let judgment = match &cached {
        Some(judgment) => judgment.clone(),
        None => type_message(state, journal, &history, record.content())?,
    };
    verbatim::apply(&mut history, record.content(), reference.clone(), &judgment)?;
    if cached.is_none() {
        journal.append(&json!({"kind":"verbatim_decision","source":"user_verbatim","text":record.content(),"source_ref":reference,"judgment":judgment}))?;
    }
    let mut projection = ActiveProjection::from_history(&history, ProjectionConfig::default())
        .map_err(|e| e.to_string())?;
    projection.generated_by.component = "kah.verbatim".into();
    persist(
        &state.binding.project,
        &source,
        previous.as_ref(),
        &history,
        &projection,
    )?;
    state.run_id = Some(source.run_id().to_string());
    journal.append(&json!({"kind":"intent_update","source":"user_verbatim","text":record.content(),"source_ref":reference,"revision":projection.revision}))?;
    Ok(projection)
}

fn type_message(
    state: &mut State,
    journal: &mut Store,
    history: &History,
    text: &str,
) -> Result<judge::Judgment, String> {
    let items = history
        .intents()
        .values()
        .filter(|i| i.is_active())
        .map(|i| (i.id.to_string(), i.text.clone()))
        .collect::<Vec<_>>();
    let questions = judge::questions::steering(&items);
    let prior_goal = (history.records().len() > 1).then(|| history.goal().text.as_str());
    let input =
        json!({"prior_intent":{"goal":prior_goal,"items":items},"event":{"user_text":text}});
    super::judgment::evaluate(state, journal, &input, &questions, "steering")
}

fn persist(
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
