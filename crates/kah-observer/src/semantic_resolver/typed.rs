use super::*;

/// One bounded, schema-constrained native call for the typed Judge adapter.
pub fn judge_call(
    root: &Path,
    config: CompactorConfig<'_>,
    schema: Value,
    prompt: &str,
) -> Result<(Value, ResolverMetadata), ResolverFailure> {
    let mut attempt = Attempt::new(config, 1);
    let call = attempt.record(invoke_codex_request(root, config, schema, prompt))?;
    let value = serde_json::from_str(&call.final_message).map_err(|_| {
        attempt.failure = Some("JUDGE_RESPONSE_JSON".into());
        attempt.error(ResolverFailureKind::InvalidProposal)
    })?;
    Ok((value, attempt.metadata()))
}
