//! Diagnose a saved fixture's decision without another worker or state transition.
use kah_core::run_store::RunStore;
use kah_observer::{engine, session::Store};
use kah_run::{store, worktree::Worktree};
use std::{env, fs, path::PathBuf};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let id = env::args().nth(1).ok_or("run id required")?;
    let base = RunStore::default_state_root()?;
    let record = store::Store::read(&base.join("runs"), &id)?;
    let directory = base.join("runs").join(&id);
    let settings = kah_cli::unattended_setup::settings(&directory)?;
    let tree = Worktree::load(&directory.join("worktree.json"))?;
    let state = Store::read(&base, &id)?.ok_or("observer state absent")?;
    let projection = engine::projection(&state)?.ok_or("projection absent")?;
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("report.json"))?)?;
    let receipt = serde_json::from_value(report["tests"].clone())?;
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME absent")?);
    let completion = kah_cli::unattended_observer::complete(
        &home,
        &settings,
        &projection,
        &[],
        &tree.diff_evidence(24 * 1024)?,
        &tree.changed_paths()?,
        &receipt,
    )?;
    println!(
        "{}",
        serde_json::json!({"diagnosticOnly":true,"runStatusUnchanged":record.control.status(),"complete":completion.complete,"judgments":completion.judgments})
    );
    Ok(())
}
