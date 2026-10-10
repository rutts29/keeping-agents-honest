//! Host-side Claude hooks supply source-bound snapshots to the existing Jev engine.
use kah_core::run_store::RunStore;
use kah_observer::{
    engine,
    session::{Binding, State, Store},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub fn run() {
    let result = serde_json::from_reader::<_, Value>(std::io::stdin())
        .map_err(|_| "CLAUDE_HOOK_JSON stage=hook: invalid hook input".into())
        .and_then(|input| {
            crate::herdr::report_claude_input(&input);
            let mut output = handle(&input)?;
            crate::observer_review::pause::annotate(&input, &mut output)
                .map_err(|e| e.to_string())?;
            crate::observer_notice::annotate(&input, &mut output).map_err(|e| e.to_string())?;
            Ok(output)
        });
    println!(
        "{}",
        result
            .unwrap_or_else(|e: String| json!({"systemMessage":format!("kah observer gap: {e}")}))
    );
}

pub fn handle(input: &Value) -> Result<Value, String> {
    if input.get("agent_id").is_some() {
        return Ok(json!({}));
    }
    let config =
        env::var("KAH_CLAUDE_BINDING").map_err(|_| "CLAUDE_BINDING stage=hook: binding absent")?;
    let mut binding: Binding =
        serde_json::from_str(&config).map_err(|_| "CLAUDE_BINDING stage=hook: invalid binding")?;
    binding.session = input["session_id"]
        .as_str()
        .ok_or("CLAUDE_SESSION stage=hook: session absent")?
        .into();
    crate::native_binding::claude(input, &binding.project)?;
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    crate::observer_hook::initialize(&base, &binding)?;
    let mut notice = None;
    let outcome = match early(input, &binding) {
        Ok(Some(output)) if !crate::observer_hook::is_observation_notice(input, &output) => {
            return Ok(output);
        }
        Ok(output) => {
            notice = output;
            observe(input, &binding)
        }
        Err(error) => Err(error),
    };
    match outcome {
        Ok(mut outcome) => {
            crate::observer_review::dispatch::outcome(&binding, &mut outcome)
                .map_err(|e| e.to_string())?;
            if let Some(reminder) = outcome.reminder {
                return Ok(json!({"decision":"block","reason":reminder.text}));
            }
            let mut output = json!({});
            if let Some(projection) = outcome.projection
                && input["hook_event_name"] != "Stop"
            {
                output["hookSpecificOutput"] = json!({"hookEventName":input["hook_event_name"],
                    "additionalContext":crate::observer_hook::context(&projection.rendered_prompt())});
            }
            if let Some(gap) = outcome.gap {
                output["systemMessage"] = format!("kah observer gap: {gap}").into();
            }
            if let Err(error) = crate::interactive_name::claude(&base, &binding, input, &mut output)
            {
                eprintln!("{error}");
            }
            Ok(crate::in_turn::merge(output, notice.unwrap_or(json!({}))))
        }
        Err(error) => {
            let mut store = Store::open(&base, &binding.session, Duration::from_secs(2))?;
            let mut state = store.load()?.unwrap_or_else(|| State::new(binding));
            let code = error
                .split_whitespace()
                .next()
                .unwrap_or("CLAUDE_SOURCE_GAP");
            state.fail(code, &error);
            store.save(&state)?;
            Err(error)
        }
    }
}

fn observe(input: &Value, binding: &Binding) -> Result<engine::Outcome, String> {
    validate_cwd(input, binding)?;
    let mut snapshot = snapshot(input, binding)?;
    crate::claude_transcript::register_hook(&binding.project, input).map_err(|e| e.to_string())?;
    // Non-Stop hooks synchronize intent, not proposed or historical actions.
    // Judging before execution can suppress the eventual turn-end reminder.
    if input["hook_event_name"] != "Stop" {
        snapshot.actions.clear();
        snapshot.action_digest = format!("sha256:{:x}", Sha256::digest(b"[]"));
    }
    let turn = input["prompt_id"].as_str().unwrap_or("session");
    crate::observer_intake::snapshot(
        binding,
        turn,
        input["hook_event_name"] == "Stop" && input["stop_hook_active"] != true,
        snapshot,
    )
}

fn early(input: &Value, binding: &Binding) -> Result<Option<Value>, String> {
    validate_cwd(input, binding)?;
    let tool = if crate::in_turn::event(input) && crate::observer_intake::legacy_checks(binding)? {
        Some(crate::in_turn::claude(binding, input)?)
    } else {
        None
    };
    crate::relay_hook::handle(binding, input, "claude").map_err(|e| e.to_string())?;
    if let Some(output) = crate::observer_review::dispatch::register(binding, input, "claude")
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(crate::in_turn::merge(
            output,
            tool.unwrap_or(json!({})),
        )));
    }
    if let Some(context) =
        crate::dispatch::startup_context(binding, input, "claude").map_err(|e| e.to_string())?
    {
        return Ok(Some(context));
    }
    crate::dispatch::receive(binding, input, "claude").map_err(|e| e.to_string())?;
    if tool.is_some() {
        return Ok(tool);
    }
    if crate::in_turn::event(input) {
        return Ok(Some(json!({})));
    }
    crate::in_turn_worker::pending(binding, input)
}

pub(crate) fn validate_cwd(input: &Value, binding: &Binding) -> Result<(), String> {
    let cwd = Path::new(
        input["cwd"]
            .as_str()
            .ok_or("CLAUDE_CWD stage=hook: cwd absent")?,
    )
    .canonicalize()
    .map_err(|e| format!("CLAUDE_CWD stage=hook: {e}"))?;
    if !cwd.starts_with(&binding.project) {
        return Err("CLAUDE_SCOPE stage=hook: cwd outside bound project".into());
    }
    Ok(())
}

pub(crate) fn snapshot(
    input: &Value,
    binding: &Binding,
) -> Result<kah_core::observation::ObservationSnapshot, String> {
    let required = if input["hook_event_name"] == "Stop" {
        Some(
            input["prompt_id"]
                .as_str()
                .ok_or("CLAUDE_PROMPT_ID stage=hook: Stop lacks prompt identity")?,
        )
    } else {
        None
    };
    for attempt in 0..4 {
        let text = transcript(input, binding)?;
        let result =
            kah_claude::transcript::parse_turn(&text, &binding.session, &binding.project, required);
        if attempt < 3
            && result.as_ref().is_err_and(|e| {
                e.starts_with("CLAUDE_SOURCE_PENDING") || e.starts_with("CLAUDE_SOURCE_JSON")
            })
        {
            std::thread::sleep(Duration::from_millis(50));
        } else {
            return result;
        }
    }
    unreachable!("last attempt returns")
}

fn transcript(input: &Value, binding: &Binding) -> Result<String, String> {
    let path = PathBuf::from(
        input["transcript_path"]
            .as_str()
            .ok_or("CLAUDE_TRANSCRIPT stage=hook: path absent")?,
    );
    let home = env::var_os("HOME").ok_or("CLAUDE_HOME stage=hook")?;
    let directory = kah_claude::paths::config_root(Path::new(&home)).join("projects");
    if !path.starts_with(&directory)
        || path.file_stem().and_then(|s| s.to_str()) != Some(&binding.session)
    {
        return Err("CLAUDE_TRANSCRIPT_PATH stage=hook: unexpected native transcript path".into());
    }
    let canonical = match path.canonicalize() {
        Ok(path) => path,
        Err(e)
            if e.kind() == std::io::ErrorKind::NotFound && input["hook_event_name"] != "Stop" =>
        {
            return Ok(String::new());
        }
        Err(e) => return Err(format!("CLAUDE_TRANSCRIPT_READ stage=hook: {e}")),
    };
    if !canonical.starts_with(
        directory
            .canonicalize()
            .map_err(|e| format!("CLAUDE_TRANSCRIPT_ROOT stage=hook: {e}"))?,
    ) {
        return Err(
            "CLAUDE_TRANSCRIPT_PATH stage=hook: transcript symlink escaped native storage".into(),
        );
    }
    let mut text = String::new();
    fs::File::open(canonical)
        .map_err(|e| format!("CLAUDE_TRANSCRIPT_READ stage=hook: {e}"))?
        .read_to_string(&mut text)
        .map_err(|e| format!("CLAUDE_TRANSCRIPT_READ stage=hook: {e}"))?;
    Ok(text)
}
