use super::*;

pub(super) fn reconcile_provider(enrollment: &Enrollment) -> Result<LockedState, String> {
    let imported = codex_source::inspect(
        enrollment.repository(),
        SessionSelection::Explicit {
            id: enrollment.session_id(),
            allow_ancestor_cwd: enrollment.allow_ancestor_cwd(),
        },
    )
    .map_err(|error| error.to_string())?;
    let bundle = imported
        .neutral_bundle()
        .map_err(|error| error.to_string())?;
    let source = InspectSource::from_bundle(&bundle).map_err(|error| error.to_string())?;
    if source.run_id().as_str() != enrollment.run_id() {
        return Err("KEEPER_OBSERVED_CODEX_SESSION_DOES_NOT_MATCH_ITS_ENROLLED stage=keeper: observed Codex session does not match its enrolled run".to_owned());
    }
    let mut state = open_state(enrollment)?;
    let accepted = state
        .recovered
        .source_cursor
        .as_ref()
        .ok_or_else(|| "KEEPER_ATTACHED_RUN_HAS_NO_ACCEPTED_SOURCE_CURSOR stage=keeper: attached run has no accepted source cursor".to_owned())?;
    match accepted
        .compare(source.cursor())
        .map_err(|error| error.to_string())?
    {
        SourceCursorComparison::Current => {}
        SourceCursorComparison::NewRecords(records) => {
            reconcile_delta(
                enrollment,
                &bundle,
                &imported,
                &source,
                &mut state,
                records.len(),
            )?;
        }
        SourceCursorComparison::Stale { .. } => {
            return Err("KEEPER_CODEX_SOURCE_IS_BEHIND_THE_ACCEPTED_SESSION_LEDGER stage=keeper: Codex source is behind the accepted session ledger".to_owned());
        }
        SourceCursorComparison::Rewrite { .. } => {
            return Err("KEEPER_ACCEPTED_CODEX_SOURCE_HISTORY_WAS_REWRITTEN stage=keeper: accepted Codex source history was rewritten".to_owned());
        }
        SourceCursorComparison::SessionMismatch => {
            return Err("KEEPER_OBSERVED_CODEX_SOURCE_BELONGS_TO_A_DIFFERENT_SESSION stage=keeper: observed Codex source belongs to a different session".to_owned());
        }
    }
    state.native_goal_mismatch = bundle
        .native_goal()
        .text()
        .is_some_and(|goal| goal != state.recovered.projection.goal.text);
    Ok(state)
}

fn reconcile_delta(
    enrollment: &Enrollment,
    bundle: &NeutralSessionBundle,
    imported: &codex_source::ImportedSession,
    source: &InspectSource,
    state: &mut LockedState,
    new_record_count: usize,
) -> Result<(), String> {
    let accepted_count = source.cursor().accepted_record_count() - new_record_count;
    let unseen = unseen_records(&state.store, bundle, accepted_count)?;
    let has_unseen_authority = unseen
        .iter()
        .any(|record| matches!(record.role(), SourceRole::User));
    if !has_unseen_authority {
        codex_source::verify_unchanged(enrollment.repository(), imported)
            .map_err(|error| error.to_string())?;
        state
            .store
            .commit_projection_with_source_cursor(&state.recovered.projection, source.cursor())
            .map_err(|error| error.to_string())?;
        state.recovered.source_cursor = Some(source.cursor().clone());
    } else {
        let delta = NeutralSessionBundle::from_records_with_native_goal(
            bundle.source().provider(),
            bundle.source().session_ref_private(),
            bundle.source().repository_digest(),
            bundle.native_goal().clone(),
            unseen,
        )
        .map_err(|error| error.to_string())?;
        let config = enrollment
            .keeper_config()
            .map_err(|error| error.to_string())?;
        let resolution = semantic_resolver::resolve_incremental(
            enrollment.repository(),
            &state.recovered.history,
            &state.recovered.projection,
            &delta,
            config,
            ProjectionConfig::new(state.recovered.projection.overflow.budget),
        )
        .map_err(|error| format!("KEEPER_KEEPER_COULD_NOT_RECONCILE_MISSED_STEERING stage=keeper: keeper could not reconcile missed steering: {error}"))?;
        codex_source::verify_unchanged(enrollment.repository(), imported)
            .map_err(|error| error.to_string())?;
        commit_resolution(state, resolution, source.cursor())?;
    }
    Ok(())
}

fn unseen_records(
    store: &RunStore,
    bundle: &NeutralSessionBundle,
    accepted_count: usize,
) -> Result<Vec<BundleRecord>, String> {
    let receipts = HookEventStore::open(store).map_err(|error| error.to_string())?;
    let mut unseen = Vec::new();
    for record in &bundle.records()[accepted_count..] {
        if matches!(record.role(), SourceRole::User)
            && receipts
                .claim_provider_record(
                    record.id(),
                    &HookEventStore::prompt_digest(record.content()),
                )
                .map_err(|error| error.to_string())?
        {
            continue;
        }
        unseen.push(record.clone());
    }
    Ok(unseen)
}
