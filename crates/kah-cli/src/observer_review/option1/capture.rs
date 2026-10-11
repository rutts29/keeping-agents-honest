//! Exact Git endpoints through the existing Mine/gVisor read-only boundary.
use super::super::{capture, state::State};
use crate::Result;
use serde_json::Value;
use std::{path::Path, process::Command};
pub fn cli(base: &str, head: &str) -> Result<()> {
    let value = super::super::commit_context::exact(
        &std::env::current_dir()?,
        (base != "empty").then_some(base),
        head,
    )?;
    println!("{value}");
    Ok(())
}
pub fn project(home: &Path, state: &State, base: Option<&str>, head: &str) -> Result<Value> {
    let args = vec![
        "__review-commit-context".into(),
        base.unwrap_or("empty").into(),
        head.into(),
    ];
    execute(home, state, &args)
}
pub fn ancestor(home: &Path, state: &State, from: &str, to: &str) -> Result<bool> {
    let value = execute(
        home,
        state,
        &["__review-commit-ancestor".into(), from.into(), to.into()],
    )?;
    value["ancestor"]
        .as_bool()
        .ok_or_else(|| "COMMIT_ANCESTRY_READ stage=capture".into())
}
fn execute(home: &Path, state: &State, args: &[String]) -> Result<Value> {
    let output = if state.worker.project.starts_with(home.join("repos/mine")) {
        Command::new(state.observer.workspace.join("bin/kah"))
            .args(args)
            .current_dir(&state.worker.project)
            .output()?
    } else {
        capture::restricted_context(home, state, args)?
    };
    if !output.status.success() {
        return Err(format!(
            "COMMIT_CONTEXT_READ stage=capture: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
