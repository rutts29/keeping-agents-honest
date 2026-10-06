//! Bundle the live repository only in gVisor; host checks use fsck-imported
//! objects and the same fresh-checkout verifier as unattended delivery.
use super::*;
use kah_sandbox::{Domain, LaunchOptions, Project};
use std::{fs, os::unix::fs::OpenOptionsExt, process::Stdio};

pub(super) fn execute(
    root: &Path,
    state: &super::super::state::State,
    round: &followup::VerificationRound,
    original: &DeliveryProof,
) -> Result<DeliveryProof> {
    if !original.passes(&round.binding.head) {
        return Err("REVIEW_FOLLOWUP_DELIVERY stage=verification".into());
    }
    let home =
        std::path::PathBuf::from(std::env::var_os("HOME").ok_or("REVIEW_HOME stage=verification")?);
    let directory = root
        .join("gate-followups")
        .join(format!("{}-checks", round.id));
    kah_core::run_store::ensure_private_directory(&directory)?;
    let bundle = directory.join("candidate.bundle");
    capture_bundle(&home, state, &bundle)?;
    let candidate = kah_run::worktree::Worktree::from_bundle(
        &state.worker.project,
        &directory.join("candidate"),
        &bundle,
        &round.binding.head,
        &format!("run-verify-{}", &round.id[9..25]),
    )?;
    let options = options(&state.worker.session)?;
    let mut proof = DeliveryProof {
        head: round.binding.head.clone(),
        fresh_checkout: true,
        tests: vec![],
        artifacts: original.artifacts.clone(),
    };
    for (index, test) in original.tests.iter().enumerate() {
        let (receipt, changed) = crate::exact_verification::execute(
            &home,
            &candidate,
            &round.binding.head,
            options,
            &test.command,
            &directory.join(format!("test-{index}.log")),
            || Ok(false),
        )?;
        proof.tests.push(TestReceipt {
            command: receipt.command,
            commit: receipt.commit,
            exit_code: receipt.exit_code,
            success: receipt.success && !changed,
            output: receipt.output,
        });
    }
    // Retain bundle, candidate and logs as exact evidence. Native model/tool
    // processes never receive write access to these private controller files.
    super::super::source_store::save(&directory, "proof.json", &serde_json::to_value(&proof)?)?;
    Ok(proof)
}
fn options(worker: &str) -> Result<LaunchOptions> {
    let saved = crate::session_policy::saved(
        &kah_core::run_store::RunStore::default_state_root()?,
        worker,
    )?;
    Ok(LaunchOptions {
        online: saved.is_some_and(|p| p.policy.ends_with("-online")),
        yolo: false,
    })
}
fn capture_bundle(home: &Path, state: &super::super::state::State, target: &Path) -> Result<()> {
    let project = Project {
        domain: Domain::Research,
        root: state.worker.project.clone(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let git = super::super::capture::worktree_git(state)?;
    let executor = crate::native_setup::executor(
        home,
        &project,
        options(&state.worker.session)?,
        git.as_deref(),
    )?
    .read_only_project();
    let root = &project.root;
    let args = vec![
        "/usr/bin/env".into(),
        "-i".into(),
        "PATH=/usr/bin:/bin".into(),
        "GIT_CONFIG_NOSYSTEM=1".into(),
        "GIT_CONFIG_GLOBAL=/dev/null".into(),
        "GIT_NO_REPLACE_OBJECTS=1".into(),
        "GIT_TERMINAL_PROMPT=0".into(),
        "/usr/bin/git".into(),
        "-c".into(),
        format!("safe.directory={}", root.display()),
        "-c".into(),
        "core.hooksPath=/dev/null".into(),
        "-c".into(),
        "core.fsmonitor=false".into(),
        "bundle".into(),
        "create".into(),
        "-".into(),
        "HEAD".into(),
    ];
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(target)?;
    let output = executor
        .command(root, &args)?
        .stdin(Stdio::null())
        .stdout(file)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "REVIEW_FOLLOWUP_BUNDLE stage=verification: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}
