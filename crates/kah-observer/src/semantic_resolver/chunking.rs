use super::*;

pub fn initial_chunk_count(bundle: &NeutralSessionBundle) -> Result<usize, String> {
    initial_chunks(bundle)
        .map(|chunks| chunks.len())
        .map_err(|failure| failure.code().to_owned())
}

/// Bootstrap in user-message order. Only the first message defines the initial
/// projection; later questions can use the existing no-change transition.
pub fn resolve_initial(
    root: &Path,
    bundle: &NeutralSessionBundle,
    config: CompactorConfig,
    projection_config: ProjectionConfig,
) -> Result<InspectResolution, ResolverFailure> {
    let chunks = initial_chunks(bundle).map_err(|failure| ResolverFailure {
        code: failure.code().into(),
        stage: "proposal_validation",
        kind: ResolverFailureKind::InvalidProposal,
        metadata: Box::new(metadata(
            config,
            0,
            0,
            ResolverUsage::default(),
            Some(failure.code().to_owned()),
            &[],
        )),
    })?;
    let mut chunks = chunks.into_iter();
    let first = chunks
        .next()
        .expect("a valid bundle has at least one chunk");
    let mut resolution = resolve(root, &first, config, projection_config)?;
    for delta in chunks {
        let prior_metadata = resolution.metadata.clone();
        resolution = match resolve_incremental(
            root,
            &resolution.history,
            &resolution.projection,
            &delta,
            config,
            projection_config,
        ) {
            Ok(mut next) => {
                combine_metadata(&mut next.metadata, &prior_metadata);
                next
            }
            Err(mut failure) => {
                combine_metadata(&mut failure.metadata, &prior_metadata);
                return Err(failure);
            }
        };
    }
    Ok(resolution)
}

pub(super) fn initial_chunks(
    bundle: &NeutralSessionBundle,
) -> Result<Vec<NeutralSessionBundle>, ValidationFailure> {
    bundle
        .validate_for_projection()
        .map_err(|_| ValidationFailure::Source)?;
    let semantic_records = semantic_records(bundle.records())?;
    let mut units = Vec::<Vec<BundleRecord>>::new();
    let mut pending = Vec::<BundleRecord>::new();
    for record in semantic_records {
        let authoritative = record.is_authoritative();
        pending.push(record);
        if authoritative {
            units.push(std::mem::take(&mut pending));
        }
    }
    if !pending.is_empty() {
        units
            .last_mut()
            .ok_or(ValidationFailure::Source)?
            .append(&mut pending);
    }
    let mut chunks = Vec::<Vec<BundleRecord>>::new();
    for mut unit in units {
        if initial_records_bytes(&unit)? > MAX_INCREMENTAL_DELTA_PROMPT_BYTES {
            unit.retain(BundleRecord::is_authoritative);
            if initial_records_bytes(&unit)? > MAX_INCREMENTAL_DELTA_PROMPT_BYTES {
                return Err(ValidationFailure::DeltaBound);
            }
        }
        chunks.push(unit);
    }
    chunks
        .into_iter()
        .map(|records| {
            NeutralSessionBundle::from_records_with_native_goal(
                bundle.source().provider(),
                bundle.source().session_ref_private(),
                bundle.source().repository_digest(),
                bundle.native_goal().clone(),
                records,
            )
            .map_err(|_| ValidationFailure::Source)
        })
        .collect()
}

pub(super) fn semantic_records(
    records: &[BundleRecord],
) -> Result<Vec<BundleRecord>, ValidationFailure> {
    let mut semantic = Vec::new();
    let mut opaque = Vec::new();
    for record in records {
        if record.is_authoritative() {
            flush_opaque_evidence(&mut opaque, &mut semantic)?;
            semantic.push(record.clone());
        } else {
            opaque.push(record);
        }
    }
    flush_opaque_evidence(&mut opaque, &mut semantic)?;
    Ok(semantic)
}

pub(super) fn flush_opaque_evidence(
    opaque: &mut Vec<&BundleRecord>,
    semantic: &mut Vec<BundleRecord>,
) -> Result<(), ValidationFailure> {
    if opaque.is_empty() {
        return Ok(());
    }
    let mut digest = Sha256::new();
    let mut assistant_records = 0_usize;
    let mut tool_records = 0_usize;
    let mut system_observation_records = 0_usize;
    let mut compaction_boundaries = 0_usize;
    for record in opaque.iter() {
        digest.update(record.id().as_bytes());
        digest.update([0]);
        digest.update(record.content_digest().as_bytes());
        digest.update([0]);
        match record.role() {
            kah_core::intent_history::SourceRole::Assistant => assistant_records += 1,
            kah_core::intent_history::SourceRole::Tool => tool_records += 1,
            kah_core::intent_history::SourceRole::SystemObservation => {
                system_observation_records += 1;
                if serde_json::from_str::<Value>(record.content())
                    .ok()
                    .and_then(|value| value.get("compaction_boundary").cloned())
                    == Some(Value::Bool(true))
                {
                    compaction_boundaries += 1;
                }
            }
            kah_core::intent_history::SourceRole::User => unreachable!("opaque batch is non-user"),
        }
    }
    let digest = format!("{:x}", digest.finalize());
    let id = format!("opaque-batch:{}", &digest[..24]);
    let content = serde_json::to_string(&json!({
        "provider_type":"opaqueEvidenceBatch",
        "record_count":opaque.len(),
        "assistant_records":assistant_records,
        "tool_records":tool_records,
        "system_observation_records":system_observation_records,
        "compaction_boundaries":compaction_boundaries,
        "records_digest":format!("sha256:{digest}"),
    }))
    .map_err(|_| ValidationFailure::Source)?;
    semantic.push(
        BundleRecord::new(
            id,
            kah_core::intent_history::SourceRole::SystemObservation,
            content,
        )
        .map_err(|_| ValidationFailure::Source)?,
    );
    opaque.clear();
    Ok(())
}

pub(super) fn initial_records_bytes(records: &[BundleRecord]) -> Result<usize, ValidationFailure> {
    let delta_records = records
        .iter()
        .map(|record| {
            json!({
                "id":record.id(),
                "role":record.role(),
                "content":record.content(),
            })
        })
        .collect::<Vec<_>>();
    let source_map = records
        .iter()
        .map(|record| {
            json!({
                "id":record.id(),
                "role":record.role(),
                "content_digest":record.content_digest(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&json!({"delta_records":delta_records,"source_map":source_map}))
        .map(|value| value.len())
        .map_err(|_| ValidationFailure::DeltaBound)
}

pub(super) fn combine_metadata(current: &mut ResolverMetadata, prior: &ResolverMetadata) {
    current.usage_complete &= prior.usage_complete;
    current.calls = current.calls.saturating_add(prior.calls);
    current.elapsed_ms = current.elapsed_ms.saturating_add(prior.elapsed_ms);
    current.usage.input_tokens = current
        .usage
        .input_tokens
        .saturating_add(prior.usage.input_tokens);
    current.usage.cached_input_tokens = current
        .usage
        .cached_input_tokens
        .saturating_add(prior.usage.cached_input_tokens);
    current.usage.output_tokens = current
        .usage
        .output_tokens
        .saturating_add(prior.usage.output_tokens);
    current.usage.reasoning_output_tokens = current
        .usage
        .reasoning_output_tokens
        .saturating_add(prior.usage.reasoning_output_tokens);
    let mut artifacts = prior.artifact_ids.clone();
    artifacts.append(&mut current.artifact_ids);
    current.artifact_ids = artifacts;
    if current.last_validation_failure.is_none() {
        current.last_validation_failure = prior.last_validation_failure.clone();
    }
}
