use super::*;

pub fn resolve(
    root: &Path,
    bundle: &NeutralSessionBundle,
    config: CompactorConfig,
    projection_config: ProjectionConfig,
) -> Result<InspectResolution, ResolverFailure> {
    let mut attempt = Attempt::new(config, PROMPT_SCHEMA_VERSION);
    let mut repair = None;
    for _ in 0..2 {
        let call = attempt.record(invoke_codex(root, bundle, config, repair.as_ref()))?;
        match validated_initial(bundle, &call.final_message, projection_config) {
            Ok((history, projection, goal)) => {
                return Ok(attempt.accept(
                    history,
                    projection,
                    observe_native_goal(bundle.native_goal(), &goal),
                    None,
                ));
            }
            Err(context) => {
                attempt.failure = Some(context.failure.clone());
                repair = Some(context);
            }
        }
    }
    Err(attempt.error(ResolverFailureKind::InvalidProposal))
}

fn validated_initial(
    bundle: &NeutralSessionBundle,
    message: &str,
    config: ProjectionConfig,
) -> Result<(History, ActiveProjection, String), RepairContext> {
    let value: Value = serde_json::from_str(message).map_err(|_| RepairContext {
        failure: "malformed_proposal_json".into(),
        previous_proposal: None,
    })?;
    let proposal: ProjectionProposal =
        serde_json::from_value(value.clone()).map_err(|_| RepairContext {
            failure: "proposal_deserialization".into(),
            previous_proposal: Some(value.clone()),
        })?;
    let goal = proposal.goal.text.clone();
    validate_and_project(bundle, proposal, config)
        .map(|(history, projection)| (history, projection, goal))
        .map_err(|failure| RepairContext {
            failure: failure.code().to_owned(),
            previous_proposal: Some(value),
        })
}
