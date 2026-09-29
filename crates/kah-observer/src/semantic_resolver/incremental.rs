use super::*;

/// Fold one bounded chronological source delta into accepted semantic state.
///
/// The immutable history is used only by deterministic validation. The model
/// receives the serialized active projection, the new delta, and its source
/// map; it never receives prior source records or history events.
#[allow(dead_code)]
pub fn resolve_incremental(
    root: &Path,
    history: &History,
    projection: &ActiveProjection,
    delta: &NeutralSessionBundle,
    config: CompactorConfig,
    projection_config: ProjectionConfig,
) -> Result<InspectResolution, ResolverFailure> {
    if let Err(failure) = incremental_prompt(history, projection, delta, None) {
        return Err(ResolverFailure {
            code: failure.code().into(),
            stage: "proposal_validation",
            kind: ResolverFailureKind::InvalidProposal,
            metadata: Box::new(metadata_for_prompt(
                config,
                0,
                0,
                ResolverUsage::default(),
                Some(failure.code().to_owned()),
                &[],
                INCREMENTAL_PROMPT_SCHEMA_VERSION,
            )),
        });
    }
    resolve_incremental_with(
        history,
        projection,
        delta,
        config,
        projection_config,
        |repair_failure| {
            invoke_codex_incremental(root, history, projection, delta, config, repair_failure)
        },
    )
}

pub(super) fn resolve_incremental_with<F>(
    history: &History,
    projection: &ActiveProjection,
    delta: &NeutralSessionBundle,
    config: CompactorConfig,
    projection_config: ProjectionConfig,
    mut invoke: F,
) -> Result<InspectResolution, ResolverFailure>
where
    F: FnMut(Option<&str>) -> Result<CodexCall, ExecutionFailure>,
{
    let mut attempt = Attempt::new(config, INCREMENTAL_PROMPT_SCHEMA_VERSION);
    for _ in 0..2 {
        let result = invoke(attempt.failure.as_deref());
        let call = attempt.record(result)?;
        let proposal = serde_json::from_str::<Value>(&call.final_message)
            .ok()
            .and_then(|value| serde_json::from_value::<IncrementalProjectionProposal>(value).ok());
        let Some(proposal) = proposal else {
            attempt.failure = Some("proposal_deserialization".to_owned());
            continue;
        };
        match validate_incremental_and_project(
            history,
            projection,
            delta,
            proposal,
            projection_config,
        ) {
            Ok((next, projected, goal_change)) => {
                return Ok(attempt.accept(
                    next,
                    projected,
                    observe_native_goal(delta.native_goal(), &history.goal().text),
                    goal_change,
                ));
            }
            Err(failure) => attempt.failure = Some(failure.code().to_owned()),
        }
    }
    Err(attempt.error(ResolverFailureKind::InvalidProposal))
}
