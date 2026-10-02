use crate::Result;
use kah_core::run_store::RunStore;
use kah_observer::session::{State, Store};
use serde_json::json;
use std::{fs, time::Duration};

pub fn run(args: Vec<String>) -> Result<()> {
    let ledger = args[0] == "ledger";
    if ledger
        && args.iter().any(|s| {
            matches!(
                s.as_str(),
                "--review" | "--confirm" | "--dismiss" | "--promote"
            )
        })
    {
        return crate::review_cli::run(&args);
    }
    let id = args.iter().skip(1).find(|s| s.as_str() != "--json");
    let json_output = args.iter().any(|s| s == "--json");
    let base = RunStore::default_state_root()?;
    let sessions = load_sessions(&base, id)?;
    let runs = if !ledger && id.is_none() {
        kah_run::commands::list(&base.join("runs"))?
    } else {
        vec![]
    };
    if json_output {
        let data = sessions
            .iter()
            .map(|s| json_session(s, ledger))
            .collect::<Result<Vec<_>>>()?;
        if !ledger && id.is_none() {
            let runs = runs
                .into_iter()
                .map(|(id, record)| Ok(json!({"id":id,"record":record.public_value()?})))
                .collect::<Result<Vec<_>>>()?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({"sessions":data,"runs":runs}))?
            );
        } else {
            println!("{}", serde_json::to_string_pretty(&data)?);
        }
        return Ok(());
    }
    for state in sessions {
        print_session(&state, ledger)?;
    }
    for (id, record) in runs {
        crate::run_commands::print_status(&id, &record);
    }
    Ok(())
}

fn print_ledger(state: &State) -> Result<()> {
    let Some(id) = &state.run_id else {
        println!("Ledger: waiting for the first observation.");
        return Ok(());
    };
    let store = RunStore::open_default(&state.binding.project, id)?;
    let recovered = store.recover()?;
    // Recovered history is parsed from the canonical JSONL segments; there is
    // no duplicate markdown ledger or model-authored source record.
    println!("Goal: {}", recovered.history.goal().text);
    for item in recovered.history.intents().values() {
        println!("- {} ({}, {:?})", item.text, item.id, item.lifecycle);
        for source in &item.introduced_by {
            if let Some(relayer) = &source.relayer {
                println!("  origin: relayed; relayer: {relayer}");
            }
        }
        if item.review_required || !item.uncertain_supersedes.is_empty() {
            println!(
                "  Uncertain replacement; retained with: {:?}",
                item.uncertain_supersedes
            );
        }
    }
    println!(
        "Revision: {}; observer reminders: {}",
        recovered.projection.revision, state.reminders
    );
    for entry in observations(state)?
        .into_iter()
        .filter(|v| v["kind"] == "judgment")
    {
        println!(
            "Judgment: {} / {} — {} (question set {})",
            entry["judgment"]["backend"].as_str().unwrap_or("unknown"),
            entry["judgment"]["model"].as_str().unwrap_or("unknown"),
            entry["assessment"]["state"].as_str().unwrap_or("unknown"),
            entry["question_set"]
        );
    }

    Ok(())
}

fn print_session(state: &State, ledger: bool) -> Result<()> {
    println!(
        "{} — {}",
        state.binding.session,
        state.binding.project.display()
    );
    let status = if state.inflight.is_some() {
        "observing"
    } else if state.gap.is_some() {
        "gap"
    } else {
        "ready"
    };
    println!(
        "Observer: {status}; {} / {}; {} calls, {} input + {} output tokens",
        state.binding.model,
        state.binding.effort,
        state.usage.calls,
        state.usage.input_tokens,
        state.usage.output_tokens
    );
    if let Some(judgment) = state.judgments.last() {
        println!("Judge: {} / {}", judgment.backend, judgment.model);
        if let Some(reason) = &judgment.fallback_reason {
            println!("Judge fallback gap: {reason}");
        }
    }
    if let Some(gap) = &state.gap {
        println!("Gap: {} — {}", gap.code, gap.detail);
    }
    if state.usage.incomplete_calls > 0 {
        println!(
            "Usage incomplete for {} failed calls.",
            state.usage.incomplete_calls
        );
    }
    if ledger {
        print_ledger(state)?;
    }
    Ok(())
}

fn json_session(state: &State, ledger: bool) -> Result<serde_json::Value> {
    if !ledger {
        return Ok(serde_json::to_value(state)?);
    }
    let Some(id) = &state.run_id else {
        return Ok(json!({"session":state,"ledger":null}));
    };
    let recovered = RunStore::open_default(&state.binding.project, id)?.recover()?;
    Ok(
        json!({"session":state,"history":recovered.history.records(),
        "projection":recovered.projection,"observations":observations(state)?}),
    )
}

fn observations(state: &State) -> Result<Vec<serde_json::Value>> {
    let path = Store::directory(&RunStore::default_state_root()?, &state.binding.session)
        .join("observations.jsonl");
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    text.lines()
        .filter(|s| !s.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

pub(crate) fn load_sessions(base: &std::path::Path, id: Option<&String>) -> Result<Vec<State>> {
    let path = base.join("observer-sessions");
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let mut sessions = Vec::<State>::new();
    for entry in entries {
        let path = entry?.path().join("state.json");
        if !path.is_file() {
            continue;
        }
        let mut state: State = serde_json::from_slice(&fs::read(path)?)?;
        if id.is_some_and(|id| id != &state.binding.session) {
            continue;
        }
        if state.recover_interrupted()
            && let Ok(mut store) = Store::open(base, &state.binding.session, Duration::ZERO)
        {
            state = store.load()?.unwrap_or(state);
            if state.recover_interrupted() {
                store.save(&state)?;
                store.append(&json!({"kind":"gap","source":"observer","gap":state.gap}))?;
            }
        }
        sessions.push(state);
    }
    Ok(sessions)
}
