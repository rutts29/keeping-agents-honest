use super::*;

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
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| ExecutionFailure::new("OBSERVER_HOME", "provider_setup"))?;
    let native = env::var_os("KAH_CODEX_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/bin/codex"));
    let environment = kah_codex::observer::Environment::prepare(&home, &native)
        .map_err(|e| ExecutionFailure::new(e.code, e.stage))?;
    let mut command = environment
        .command(&kah_codex::observer::Request {
            model: config.model(),
            effort: config.reasoning(),
            schema: &files.schema,
            output: &files.final_message,
            timeout: config.timeout(),
        })
        .map_err(|e| ExecutionFailure::new(e.code, e.stage))?;
    let mut child = command
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
    if matches!(output.status.code(), Some(124 | 137)) {
        return Err(ExecutionFailure {
            code: "OBSERVER_TIMEOUT",
            stage: "provider_wait",
            artifact: Some(artifact_id),
        });
    }
    if !output.status.success() {
        return Err(ExecutionFailure::artifact("OBSERVER_EXIT", artifact_id));
    }
    let final_message = String::from_utf8(final_bytes)
        .map_err(|_| ExecutionFailure::artifact("OBSERVER_UTF8", artifact_id.clone()))?;
    let (trajectory, usage) = parse_trajectory(&output.stdout)
        .map_err(|_| ExecutionFailure::artifact("OBSERVER_TRAJECTORY", artifact_id.clone()))?;
    if !data_only_trajectory(&trajectory) {
        return Err(ExecutionFailure::artifact(
            "OBSERVER_UNEXPECTED_ACTIVITY",
            artifact_id,
        ));
    }
    Ok(CodexCall {
        final_message,
        usage,
        artifact_id,
    })
}

pub(super) fn data_only_trajectory(events: &[Value]) -> bool {
    let mut started = false;
    let mut completed = false;
    let safe = events.iter().all(|event| match event["type"].as_str() {
        Some("thread.started") => true,
        Some("turn.started") => {
            started = true;
            true
        }
        Some("turn.completed") => {
            completed = true;
            true
        }
        Some("item.started" | "item.updated" | "item.completed") => {
            matches!(
                event["item"]["type"].as_str(),
                Some("agent_message" | "reasoning")
            ) || (!started && event["item"]["type"] == "error")
        }
        _ => false,
    });
    safe && started && completed
}
