use super::*;

pub fn on_user_prompt(
    enrollment: &Enrollment,
    turn_id: &str,
    prompt: &str,
) -> Result<KeeperDecision, String> {
    let record_id = prompt_record_id(turn_id)?;
    let record = BundleRecord::new(&record_id, SourceRole::User, prompt)
        .map_err(|error| format!("KEEPER_INVALID_USER_PROMPT_FOR_KEEPER stage=keeper: invalid user prompt for keeper: {error}"))?;
    let receipt_digest = HookEventStore::prompt_digest(record.content());

    if let Some(decision) = already_received(enrollment, &record_id, &receipt_digest)? {
        return Ok(decision);
    }

    let mut state = reconcile_provider(enrollment)?;
    if let Some(reason) = blocker(&state)? {
        return Ok(KeeperDecision::Blocked {
            projection: state.recovered.projection,
            reason,
        });
    }
    let receipts = HookEventStore::open(&state.store).map_err(|error| error.to_string())?;
    if receipts
        .matches(&record_id, &receipt_digest)
        .map_err(|error| error.to_string())?
    {
        return decision_from_state(state);
    }
    if receipts
        .conflicts(&record_id, &receipt_digest)
        .map_err(|error| error.to_string())?
    {
        return Ok(KeeperDecision::Blocked {
            projection: state.recovered.projection,
            reason: "the observed Codex prompt conflicts with the hook delivery".to_owned(),
        });
    }
    let cursor = state
        .recovered
        .source_cursor
        .as_ref()
        .ok_or_else(|| "KEEPER_ATTACHED_RUN_HAS_NO_ACCEPTED_SOURCE_CURSOR stage=keeper: attached run has no accepted source cursor".to_owned())?;

    let delta = NeutralSessionBundle::from_records_with_native_goal(
        SourceProvider::Codex,
        enrollment.session_id(),
        cursor.repository_digest(),
        NativeGoal::Unknown,
        vec![record.clone()],
    )
    .map_err(|error| format!("KEEPER_COULD_NOT_CONSTRUCT_PROMPT_DELTA stage=keeper: could not construct prompt delta: {error}"))?;
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
    .map_err(|error| format!("KEEPER_KEEPER_COULD_NOT_VALIDATE_THIS_STEERING stage=keeper: keeper could not validate this steering: {error}"))?;
    let provider_cursor = cursor.clone();
    commit_resolution(&mut state, resolution, &provider_cursor)?;
    record_prompt_receipt(&state, &record_id, &receipt_digest)?;
    decision_from_state(state)
}

fn already_received(
    enrollment: &Enrollment,
    record_id: &str,
    receipt_digest: &str,
) -> Result<Option<KeeperDecision>, String> {
    let initial = open_state(enrollment)?;
    let receipts = HookEventStore::open(&initial.store).map_err(|error| error.to_string())?;
    if receipts
        .matches(record_id, receipt_digest)
        .map_err(|error| error.to_string())?
    {
        return decision_from_state(initial).map(Some);
    }
    if receipts
        .conflicts(record_id, receipt_digest)
        .map_err(|error| error.to_string())?
    {
        return Ok(Some(KeeperDecision::Blocked {
            projection: initial.recovered.projection,
            reason: "the same Codex turn ID was delivered with different prompt content".to_owned(),
        }));
    }
    Ok(None)
}

fn record_prompt_receipt(
    state: &LockedState,
    record_id: &str,
    receipt_digest: &str,
) -> Result<(), String> {
    HookEventStore::open(&state.store)
        .and_then(|store| {
            store.record(
                record_id,
                receipt_digest,
                state.recovered.projection.revision,
            )
        })
        .map_err(|error| error.to_string())
}
