//! Read source objects/refs on the host; only fsck-imported objects reach checks.
use super::*;
use kah_run::worktree::Worktree;
use std::{
    os::unix::fs::OpenOptionsExt,
    process::{Command, Stdio},
};
mod tags;
pub(super) fn load(
    root: &Path,
    state: &crate::observer_review::state::State,
    head: &str,
) -> Result<Worktree> {
    let saved = root.join("candidate.json");
    let bundle = root.join("source.bundle");
    if !bundle.exists() {
        if saved.exists() {
            return Err(
                "REVIEW_GATE_BUNDLE_MISSING stage=verification: retained candidate lost its bundle"
                    .into(),
            );
        }
        export(&state.worker.project, head, &bundle)?;
    }
    let candidate = if saved.exists() {
        Worktree::load(&saved)?
    } else {
        create(root, state, &bundle)?
    };
    tags::import(&candidate, &bundle)?;
    if !git()
        .arg("-C")
        .arg(&candidate.path)
        .args(["cat-file", "-e", &format!("{head}^{{commit}}")])
        .status()?
        .success()
    {
        return Err("REVIEW_GATE_SOURCE_MISSING stage=verification".into());
    }
    if !saved.exists() {
        candidate.save(&saved)?;
    }
    Ok(candidate)
}
fn create(
    root: &Path,
    state: &crate::observer_review::state::State,
    bundle: &Path,
) -> Result<Worktree> {
    let output = git()
        .args([
            "bundle",
            "list-heads",
            bundle
                .to_str()
                .ok_or("REVIEW_GATE_BUNDLE_PATH stage=verification")?,
            "HEAD",
        ])
        .output()?;
    let advertised = String::from_utf8(output.stdout)?;
    let advertised = advertised
        .split_whitespace()
        .next()
        .ok_or("REVIEW_GATE_BUNDLE_HEAD stage=verification")?;
    Ok(Worktree::from_bundle(
        &state.worker.project,
        &root.join("candidate"),
        bundle,
        advertised,
        &format!("run-gate-{}", crate::relay_store::nonce()),
    )?)
}
fn export(project: &Path, head: &str, bundle: &Path) -> Result<()> {
    kah_core::run_store::ensure_save_directory(
        bundle
            .parent()
            .ok_or("REVIEW_GATE_BUNDLE_PATH stage=verification")?,
    )?;
    let temporary = bundle.with_extension(format!("{}.partial", crate::relay_store::nonce()));
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    write_bundle(project, head, Stdio::from(output))?;
    fs::File::open(&temporary)?.sync_all()?;
    fs::rename(temporary, bundle)?;
    Ok(())
}
pub fn bundle_cli(head: &str) -> Result<()> {
    write_bundle(&std::env::current_dir()?, head, Stdio::inherit())
}
fn write_bundle(project: &Path, head: &str, output: Stdio) -> Result<()> {
    if !super::super::oid(head) {
        return Err("REVIEW_GATE_HEAD stage=bundle".into());
    }
    if !git()
        .arg("-C")
        .arg(project)
        .args(["cat-file", "-e", &format!("{head}^{{commit}}")])
        .status()?
        .success()
    {
        return Err("REVIEW_GATE_SOURCE_MISSING stage=bundle".into());
    }
    if !git()
        .arg("-C")
        .arg(project)
        .args(["bundle", "create", "-", "--all", "HEAD"])
        .stdout(output)
        .status()?
        .success()
    {
        return Err("REVIEW_GATE_BUNDLE stage=bundle".into());
    }
    Ok(())
}
fn git() -> Command {
    // git/git v2.53.0 Documentation/git-bundle.adoc; same object boundary as Worktree::bundle_head.
    let mut c = Command::new("/usr/bin/git");
    c.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            "-c",
            "protocol.allow=never",
        ]);
    c
}

#[cfg(test)]
#[path = "../../../../tests/observer_unit/linked_gate_bundle.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/observer_unit/tag_import.rs"]
mod tag_tests;
