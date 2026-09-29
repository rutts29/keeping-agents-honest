use super::*;

pub(super) fn observe_native_goal(
    native_goal: &NativeGoal,
    projected_goal: &str,
) -> NativeGoalObservation {
    let text_private = native_goal.text().map(str::to_owned);
    NativeGoalObservation {
        state: native_goal.state().to_owned(),
        conflicts_with_projection: text_private
            .as_deref()
            .is_some_and(|observed| observed != projected_goal),
        text_private,
    }
}

pub(super) fn metadata(
    config: CompactorConfig,
    calls: u8,
    elapsed_ms: u128,
    usage: ResolverUsage,
    last_validation_failure: Option<String>,
    artifact_ids: &[String],
) -> ResolverMetadata {
    metadata_for_prompt(
        config,
        calls,
        elapsed_ms,
        usage,
        last_validation_failure,
        artifact_ids,
        PROMPT_SCHEMA_VERSION,
    )
}

pub(super) fn metadata_for_prompt(
    config: CompactorConfig,
    calls: u8,
    elapsed_ms: u128,
    usage: ResolverUsage,
    last_validation_failure: Option<String>,
    artifact_ids: &[String],
    prompt_schema_version: u32,
) -> ResolverMetadata {
    ResolverMetadata {
        model: config.model().to_owned(),
        reasoning: config.reasoning().to_owned(),
        calls,
        elapsed_ms,
        usage,
        prompt_schema_version,
        proposal_schema_version: PROPOSAL_SCHEMA_VERSION,
        last_validation_failure,
        artifact_ids: artifact_ids.to_vec(),
    }
}
