use super::*;
use kah_core::intent_history::{Event, GoalRevision, SourceRole};

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
        .map(|locked| locked.recovered);
    if previous.is_some() {
        state.run_id = Some(source.run_id().as_str().into());
    }
    let result = if let Some(previous) = &previous {
        let cursor = previous
            .source_cursor
            .as_ref()
            .ok_or("OBSERVER_CURSOR stage=observe: accepted cursor absent")?;
        match cursor.compare(source.cursor()).map_err(|e|e.to_string())? {
            SourceCursorComparison::Current=>return Ok((previous.projection.clone(),false)),
            SourceCursorComparison::NewRecords(_)=>{
                let delta=source.delta_bundle(users,cursor.accepted_record_count()).map_err(|e|e.to_string())?;
                semantic_resolver::resolve_incremental(&state.binding.project,&previous.history,&previous.projection,&delta,config,ProjectionConfig::default())
            }
            _=>return Err("OBSERVER_SOURCE_REWRITE stage=observe: accepted user source changed or moved backwards".into()),
        }
    } else {
        semantic_resolver::resolve_initial(
            &state.binding.project,
            users,
            config,
            ProjectionConfig::default(),
        )
    };
    let mut resolution = match result {
        Ok(r) => r,
        Err(e) => {
            state.usage.record(&e.metadata);
            return Err(e.to_string());
        }
    };
    state.usage.record(&resolution.metadata);
    apply_user_goal_change(&mut resolution)?;
    persist(&state.binding.project, &source, previous, &resolution)?;
    state.run_id = Some(source.run_id().as_str().into());
    journal.append(&json!({"kind":"intent_update","source":"observer_interpretation","revision":resolution.projection.revision,"metadata":resolution.metadata}))?;
    Ok((resolution.projection, true))
}

fn apply_user_goal_change(
    resolution: &mut semantic_resolver::InspectResolution,
) -> Result<(), String> {
    if let Some(change) = resolution.goal_change.take() {
        if !change
            .source_refs
            .iter()
            .all(|s| s.role == SourceRole::User)
        {
            return Err(
                "OBSERVER_GOAL_AUTHORITY stage=observe: goal change has a non-user source".into(),
            );
        }
        let current = resolution.history.goal().revision;
        let mut goal = GoalRevision::new(current + 1, change.proposed_goal, change.source_refs);
        goal.supersedes_revision = Some(current);
        resolution
            .history
            .append(Event::GoalRevised { goal })
            .map_err(|e| e.to_string())?;
        resolution.projection =
            ActiveProjection::from_history(&resolution.history, ProjectionConfig::default())
                .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn persist(
    project: &std::path::Path,
    source: &InspectSource,
    previous: Option<kah_core::run_store::RecoveredRun>,
    resolution: &semantic_resolver::InspectResolution,
) -> Result<(), String> {
    if let Some(previous) = previous {
        let store =
            RunStore::open_default(project, source.run_id().as_str()).map_err(|e| e.to_string())?;
        for record in &resolution.history.records()[previous.history.records().len()..] {
            store
                .append_pending(record.clone())
                .map_err(|e| e.to_string())?;
        }
        store
            .commit_projection_with_source_cursor(&resolution.projection, source.cursor())
            .map_err(|e| e.to_string())?;
    } else {
        source
            .create(project, &resolution.history, &resolution.projection)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
