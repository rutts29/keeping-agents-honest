//! Offline replay/validation helper. No transport or provider dependencies.
use kah_core::{
    current_rules::{Input, RuleSet, Source},
    intent_history::History,
};
use serde_json::json;
use std::{env, fs};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.first().map(String::as_str) {
        Some("input") if args.len() >= 4 => {
            let history = History::from_json(&fs::read_to_string(&args[1])?)?;
            let standing = args[3..]
                .iter()
                .map(|path| {
                    let text = fs::read_to_string(path)?;
                    let resolved = fs::canonicalize(path)?;
                    Ok(Source::standing(
                        path,
                        &text,
                        json!({"resolved_path":resolved,"scope":"project"}),
                    ))
                })
                .collect::<Result<Vec<_>, std::io::Error>>()?;
            // Fixed example observation time, not the time these inputs were collected.
            let input = Input::from_history(&history, standing, "2026-10-11T00:00:00Z")?;
            fs::write(&args[2], serde_json::to_vec_pretty(&input)?)?;
            println!("{}", input.digest()?);
        }
        Some("validate") if args.len() == 3 => {
            let input: Input = serde_json::from_slice(&fs::read(&args[1])?)?;
            let set: RuleSet = serde_json::from_slice(&fs::read(&args[2])?)?;
            let validated = set.validate(&input, None)?;
            println!(
                "{}",
                json!({"valid":true,"set_digest":validated.digest()?,
                "input_digest":input.digest()?,"revision":input.revision,
                "rule_count":set.rules.len(),"diff_checkable":set.rules.iter().filter(|r|r.diff_checkable).count(),
                "conflicts":validated.has_conflicts(),"diff":validated.diff(None)})
            );
        }
        _ => return Err(
            "RULE_SET_ARGUMENTS stage=offline: input HISTORY OUTPUT FILE... | validate INPUT SET"
                .into(),
        ),
    }
    Ok(())
}
