use super::*;

pub(super) fn invoke_codex(
    root: &Path,
    bundle: &NeutralSessionBundle,
    config: CompactorConfig,
    repair: Option<&RepairContext>,
) -> Result<CodexCall, ExecutionFailure> {
    let prompt = prompt(bundle, repair)
        .map_err(|_| ExecutionFailure::new("OBSERVER_PROMPT", "proposal_prompt"))?;
    invoke_codex_request(root, config, proposal_schema(), &prompt)
}

pub(super) fn invoke_codex_incremental(
    root: &Path,
    history: &History,
    projection: &ActiveProjection,
    delta: &NeutralSessionBundle,
    config: CompactorConfig,
    repair_failure: Option<&str>,
) -> Result<CodexCall, ExecutionFailure> {
    let prompt = incremental_prompt(history, projection, delta, repair_failure)
        .map_err(|_| ExecutionFailure::new("OBSERVER_INCREMENTAL_PROMPT", "proposal_prompt"))?;
    invoke_codex_request(root, config, incremental_proposal_schema(), &prompt)
}

pub(super) fn invoke_codex_request(
    root: &Path,
    config: CompactorConfig,
    schema: Value,
    prompt: &str,
) -> Result<CodexCall, ExecutionFailure> {
    // Codex's documented automation boundary supports explicit sandboxing,
    // ephemeral sessions, JSONL trajectories, and schema-constrained final output.
    // Source: https://learn.chatgpt.com/docs/non-interactive-mode
    let files = TemporaryCallFiles::create()
        .map_err(|_| ExecutionFailure::new("OBSERVER_TEMP_FILES", "provider_setup"))?;
    fs::write(&files.schema, schema.to_string())
        .map_err(|_| ExecutionFailure::new("OBSERVER_SCHEMA_WRITE", "provider_setup"))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|_| ExecutionFailure::new("OBSERVER_ROOT", "provider_setup"))?;
    let artifact_store = ArtifactStore::open(&canonical_root)
        .map_err(|_| ExecutionFailure::new("OBSERVER_ARTIFACT_STORE", "provider_setup"))?;
    let program = env::var_os("KAH_CODEX_BIN").unwrap_or_else(|| "codex".into());
    let reasoning = format!("model_reasoning_effort=\"{}\"", config.reasoning());
    let mut child = Command::new(program)
        .args([
            "exec",
            "--model",
            config.model(),
            "--config",
            &reasoning,
            "--sandbox",
            "read-only",
            "--ephemeral",
            "--json",
            "--output-schema",
        ])
        .arg(&files.schema)
        .arg("--output-last-message")
        .arg(&files.final_message)
        .arg("-C")
        .arg(canonical_root)
        .arg("-")
        // Compaction uses the installed CLI's saved local authentication, not
        // an API key copied or inherited by Driftctl.
        .env_remove("OPENAI_API_KEY")
        .env_remove("CODEX_API_KEY")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ExecutionFailure::new("OBSERVER_SPAWN", "provider_start"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| ExecutionFailure::new("OBSERVER_STDIN", "provider_input"))?
        .write_all(prompt.as_bytes())
        .map_err(|_| ExecutionFailure::new("OBSERVER_INPUT_WRITE", "provider_input"))?;
    let output = child
        .wait_with_output()
        .map_err(|_| ExecutionFailure::new("OBSERVER_WAIT", "provider_wait"))?;
    decode_output(output, &files, artifact_store)
}

fn decode_output(
    output: std::process::Output,
    files: &TemporaryCallFiles,
    artifact_store: ArtifactStore,
) -> Result<CodexCall, ExecutionFailure> {
    let final_bytes = fs::read(&files.final_message).unwrap_or_default();
    let artifact_id = artifact_store
        .retain(&output.stdout, &final_bytes)
        .map_err(|_| ExecutionFailure::new("OBSERVER_ARTIFACT_WRITE", "provider_output"))?;
    if !output.status.success() {
        return Err(ExecutionFailure::artifact("OBSERVER_EXIT", artifact_id));
    }
    let final_message = String::from_utf8(final_bytes)
        .map_err(|_| ExecutionFailure::artifact("OBSERVER_UTF8", artifact_id.clone()))?;
    let (trajectory, usage) = parse_trajectory(&output.stdout)
        .map_err(|_| ExecutionFailure::artifact("OBSERVER_TRAJECTORY", artifact_id.clone()))?;
    Ok(CodexCall {
        final_message,
        usage,
        artifact_id,
        trajectory,
    })
}
