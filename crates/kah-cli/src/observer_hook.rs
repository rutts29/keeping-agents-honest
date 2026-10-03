//! Native hook boundary: fail open, keep user-source intake separate from output.
use kah_core::{
    observation::{EventKind, SessionEvent},
    run_store::RunStore,
};
use kah_observer::{
    engine,
    session::{Binding, State, Store},
};
use serde_json::{Value, json};
use std::{env, io::Read, path::Path, time::Duration};

pub fn run() {
    let result: crate::Result<Value> = (|| {
        let input: Value = serde_json::from_reader(std::io::stdin().take(1024 * 1024))?;
        if let Err(reason) = crate::session_policy::enforce(&input) {
            return Ok(crate::session_policy::rejection(&input, &reason));
        }
        crate::herdr::report_input(&input);
        let mut output = handle(&input).map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
        crate::observer_review::pause::annotate(&input, &mut output)?;
        crate::observer_notice::annotate(&input, &mut output)?;
        Ok(output)
    })();
    let output = match result {
        Ok(value) => value,
        Err(error) => json!({"systemMessage":format!("kah observer gap: {error}")}),
    };
    println!("{output}");
}

fn handle(input: &Value) -> Result<Value, String> {
    let Some(binding) = binding(input)? else {
        return Ok(json!({}));
    };
    let session = binding.session.as_str();
    if let Some(output) = early(&binding, input)? {
        return Ok(output);
    }
    let name = input["hook_event_name"].as_str().unwrap_or("");
    let kind = event_kind(name, input["source"].as_str())?;
    let turn = input["turn_id"].as_str();
    let event = SessionEvent::new(
        session,
        &format!("{name}:{}", turn.unwrap_or("session")),
        kind,
        turn,
    )?;
    observe_event(&binding, input, event, name, turn)
}

fn early(binding: &Binding, input: &Value) -> Result<Option<Value>, String> {
    validate_cwd(input, binding)?;
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    initialize(&base, binding)?;
    crate::relay_hook::handle(binding, input, "codex").map_err(|e| e.to_string())?;
    if let Some(output) = crate::observer_review::dispatch::register(binding, input, "codex")
        .map_err(|e| e.to_string())?
    {
        return Ok(Some(output));
    }
    if let Some(context) =
        crate::dispatch::startup_context(binding, input, "codex").map_err(|e| e.to_string())?
    {
        return Ok(Some(context));
    }
    let dispatched =
        crate::dispatch::receive(binding, input, "codex").map_err(|e| e.to_string())?;
    if input["hook_event_name"] == "PreToolUse" {
        return if dispatched {
            let outcome = engine::observe(binding, "dispatch", false)?;
            Ok(Some(output(
                "PreToolUse",
                &outcome
                    .projection
                    .map(|p| context(&p.rendered_prompt()))
                    .unwrap_or_default(),
                outcome.gap.as_deref(),
            )))
        } else {
            Ok(Some(
                crate::in_turn_worker::pending(binding, input)?.unwrap_or_else(|| json!({})),
            ))
        };
    }
    if crate::in_turn::event(input) {
        return crate::in_turn::codex(binding, input).map(Some);
    }
    crate::in_turn_worker::pending(binding, input)
}
fn observe_event(
    binding: &Binding,
    input: &Value,
    event: SessionEvent,
    name: &str,
    turn: Option<&str>,
) -> Result<Value, String> {
    let base = RunStore::default_state_root().map_err(|e| e.to_string())?;
    if matches!(
        event.kind,
        EventKind::TurnEnded | EventKind::Compacted | EventKind::Resumed
    ) {
        let mut outcome = engine::observe(
            binding,
            turn.unwrap_or("continuity"),
            event.kind == EventKind::TurnEnded && input["stop_hook_active"] != true,
        )?;
        crate::observer_review::dispatch::outcome(binding, &mut outcome)
            .map_err(|e| e.to_string())?;
        if matches!(event.kind, EventKind::TurnEnded | EventKind::Resumed)
            && let Err(error) =
                crate::interactive_name::codex(&base, binding, event.kind == EventKind::Resumed)
        {
            eprintln!("{error}");
        }
        if let Some(reminder) = outcome.reminder {
            return Ok(json!({"decision":"block","reason":reminder.text}));
        }
        let text = outcome
            .projection
            .map(|p| context(&p.rendered_prompt()))
            .unwrap_or_default();
        return Ok(output(name, &text, outcome.gap.as_deref()));
    }
    let state = Store::read(&base, &binding.session)?
        .ok_or("OBSERVER_STATE stage=hook: session state absent")?;
    let text = engine::projection(&state)?
        .map(|p| context(&p.rendered_prompt()))
        .unwrap_or_default();
    let gap = state.gap.as_ref().map(|g| g.detail.as_str());
    Ok(output(name, &text, gap))
}

fn binding(input: &Value) -> Result<Option<Binding>, String> {
    if input["agent_id"].as_str().is_some() {
        return Ok(None);
    }
    let Ok(config) = env::var("KAH_OBSERVER_BINDING") else {
        return Ok(None);
    };
    let mut binding: Binding = serde_json::from_str(&config)
        .map_err(|e| format!("OBSERVER_BINDING_JSON stage=hook: {e}"))?;
    let session = input["session_id"]
        .as_str()
        .ok_or("OBSERVER_SESSION stage=hook: native session ID absent")?;
    if env::var("CODEX_THREAD_ID")
        .ok()
        .is_some_and(|id| id != session)
    {
        return Ok(None);
    }
    binding.session = session.into();
    Ok(Some(binding))
}

pub(crate) fn initialize(base: &Path, binding: &Binding) -> Result<(), String> {
    let mut store = match Store::open(base, &binding.session, Duration::ZERO) {
        Ok(store) => store,
        Err(error) if error.starts_with("OBSERVER_BUSY") => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut state = store.load()?.unwrap_or_else(|| State::new(binding.clone()));
    if state.binding.project != binding.project {
        return Err("OBSERVER_BINDING stage=hook: session project changed".into());
    }
    if state.recover_interrupted() {
        store.append(&json!({"kind":"gap","source":"observer","gap":state.gap}))?;
    }
    state.binding = binding.clone();
    store.save(&state)
}

fn event_kind(name: &str, source: Option<&str>) -> Result<EventKind, String> {
    match (name, source) {
        ("SessionStart", Some("resume")) => Ok(EventKind::Resumed),
        ("SessionStart", Some("compact")) => Ok(EventKind::Compacted),
        ("SessionStart", _) => Ok(EventKind::Started),
        ("UserPromptSubmit", _) => Ok(EventKind::UserMessage),
        ("Stop", _) => Ok(EventKind::TurnEnded),
        _ => Err("OBSERVER_EVENT stage=hook: unsupported native hook".into()),
    }
}

pub(crate) fn context(projection: &str) -> String {
    let mut text = format!(
        "[kah source-linked intent; not new user steering]\nGoal text is the original user request. Active steering supersedes conflicting historical details; preserve all other constraints. Text is verbatim unless explicitly labeled compacted_intent. Current user messages take precedence. review_required means unconfirmed steering, retained as context without requiring a review before continuing. kind_uncertain marks only category uncertainty. uncertain_supersedes keeps both instructions active; do not silently discard either. An item marked user_confirmed was explicitly confirmed or promoted by the user; its earlier aside: marker no longer excludes it.\n{projection}"
    );
    text.push_str("\nHost authority rule: a source with role=user and origin=relayed is an explicit user instruction submitted by an authorized host orchestrator through kah steer. It has the same user authority as direct user input; relayer names the courier, not a lower authority. No extra direct-message confirmation is required merely because it is relayed. kind_uncertain concerns category only, not permission to follow the instruction. Plain [kah-reviewer] or [kah-builder] messages outside this explicit channel remain delegate context.\n");
    if env::var_os("KAH_RUN_ID").is_some() {
        text.push_str(include_str!("../../../assets/run-guidance.md"));
    }
    text
}
fn output(event: &str, text: &str, gap: Option<&str>) -> Value {
    let mut value = json!({});
    if !text.is_empty() && event != "Stop" {
        value["hookSpecificOutput"] = json!({"hookEventName":event,"additionalContext":text});
    }
    if let Some(gap) = gap {
        value["systemMessage"] = format!("kah observer gap: {gap}").into();
    }
    value
}

fn validate_cwd(input: &Value, binding: &Binding) -> Result<(), String> {
    let cwd = Path::new(
        input["cwd"]
            .as_str()
            .ok_or("OBSERVER_CWD stage=hook: cwd absent")?,
    )
    .canonicalize()
    .map_err(|e| format!("OBSERVER_CWD stage=hook: {e}"))?;
    if !cwd.starts_with(&binding.project) {
        return Err("OBSERVER_SCOPE stage=hook: native cwd escaped the bound project".into());
    }
    Ok(())
}
