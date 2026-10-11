//! The bundle is created in gVisor; only fsck-checked objects reach the host.
use super::*;
use kah_run::worktree::Worktree;
use kah_sandbox::{Domain, Project};
use std::{
    os::unix::fs::OpenOptionsExt,
    process::{Command, Stdio},
};
pub(super) fn load(
    home: &Path,
    root: &Path,
    state: &crate::observer_review::state::State,
    head: &str,
) -> Result<Worktree> {
    let saved = root.join("candidate.json");
    if saved.exists() {
        return Worktree::load(&saved).map_err(Into::into);
    }
    let bundle = root.join("source.bundle");
    if !bundle.exists() {
        export(home, state, head, &bundle)?;
    }
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
    let candidate = Worktree::from_bundle(
        &state.worker.project,
        &root.join("candidate"),
        &bundle,
        advertised,
        &format!("run-gate-{}", crate::relay_store::nonce()),
    )?;
    // The imported HEAD may be newer. Every verifier still selects the gate SHA.
    if !git()
        .arg("-C")
        .arg(&candidate.path)
        .args(["cat-file", "-e", &format!("{head}^{{commit}}")])
        .status()?
        .success()
    {
        return Err("REVIEW_GATE_SOURCE_MISSING stage=verification".into());
    }
    candidate.save(&saved)?;
    Ok(candidate)
}
fn export(
    home: &Path,
    state: &crate::observer_review::state::State,
    head: &str,
    bundle: &Path,
) -> Result<()> {
    let temporary = bundle.with_extension(format!("{}.partial", crate::relay_store::nonce()));
    let project = Project {
        domain: Domain::Research,
        root: state.worker.project.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let binary = state.observer.workspace.join("bin/kah");
    let executor = crate::native_setup::executor(
        home,
        &project,
        super::super::checks::options(&state.worker.session)?,
        None,
    )?
    .with_read_only_mounts(&[(binary.clone(), binary.clone())])?
    .read_only_project();
    let output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)?;
    let status = executor
        .command(
            &project.root,
            &[
                binary.to_string_lossy().into_owned(),
                "__review-gate-bundle".into(),
                head.into(),
            ],
        )?
        .stdout(output)
        .stderr(Stdio::piped())
        .output()?;
    if !status.status.success() {
        return Err(format!(
            "REVIEW_GATE_BUNDLE stage=verification: {}",
            String::from_utf8_lossy(&status.stderr)
        )
        .into());
    }
    fs::File::open(&temporary)?.sync_all()?;
    fs::rename(temporary, bundle)?;
    Ok(())
}
pub fn bundle_cli(head: &str) -> Result<()> {
    if !super::super::oid(head) {
        return Err("REVIEW_GATE_HEAD stage=bundle".into());
    }
    if !git()
        .args(["cat-file", "-e", &format!("{head}^{{commit}}")])
        .status()?
        .success()
    {
        return Err("REVIEW_GATE_SOURCE_MISSING stage=bundle".into());
    }
    if !git()
        .args(["bundle", "create", "-", "--all", "HEAD"])
        .stdout(Stdio::inherit())
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
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.pager=cat",
        ]);
    c
}
