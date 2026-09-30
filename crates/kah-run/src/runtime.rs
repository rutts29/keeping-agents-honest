//! Private controller configuration for a credential-free unattended executor.
use crate::{launch, store::Record, worktree::Worktree};
use kah_codex::overlay::{self, Policy};
use kah_sandbox::{Domain, Executor, current_uid, publish_binary};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Runtime {
    pub native: PathBuf,
    pub overlay: PathBuf,
    pub executor: Executor,
}
impl Runtime {
    pub fn prepare(
        home: &Path,
        state: &Path,
        id: &str,
        record: &Record,
        tree: &Worktree,
    ) -> Result<Self, String> {
        if record.worktree != tree.path {
            return Err("RUN_WORKTREE_BINDING stage=worker_prepare".into());
        }
        let native = home
            .join(".local/bin/codex")
            .canonicalize()
            .map_err(io_error)?;
        let public = publish_binary(
            &native,
            &PathBuf::from(format!(
                "/var/tmp/kah-runtime-{}",
                current_uid().map_err(|e| e.to_string())?
            )),
        )
        .map_err(|e| e.to_string())?;
        let image = fs::read_to_string("/usr/local/share/sbx/image-ref").map_err(io_error)?;
        let domain = Domain::ALL
            .into_iter()
            .find(|d| d.as_str() == record.domain)
            .ok_or("RUN_DOMAIN stage=worker_prepare")?;
        let executor = launch::sandbox(
            domain,
            &tree.path,
            &tree.git_dir,
            &public,
            image.trim(),
            service_uid()?,
            id,
        )?;
        let overlay = state.join("codex");
        overlay::prepare(
            &home.join(".codex"),
            &overlay,
            &Policy {
                restricted: true,
                online: false,
                root: &tree.path,
                cwd: &tree.path,
            },
        )
        .map_err(|e| e.to_string())?;
        overlay::set_executor(&overlay, "kah-unattended", &executor.args)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            native,
            overlay,
            executor,
        })
    }
    pub fn command(&self, record: &Record) -> Command {
        launch::WorkerLaunch {
            native: &self.native,
            overlay: &self.overlay,
            worktree: &record.worktree,
            model: record.model.as_deref(),
            effort: record.effort.as_deref(),
        }
        .command()
    }
    pub fn stop_executor(&self, id: &str) -> Result<(), String> {
        let output = docker(&self.executor)
            .args(["ps", "-aq", "--filter", &format!("label=dev.kah.run={id}")])
            .output()
            .map_err(io_error)?;
        if !output.status.success() {
            return Err("RUN_CONTAINER_LIST stage=worker_cleanup".into());
        }
        for container in String::from_utf8_lossy(&output.stdout).split_whitespace() {
            if !container.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err("RUN_CONTAINER_ID stage=worker_cleanup".into());
            }
            if !docker(&self.executor)
                .args(["rm", "-f", container])
                .output()
                .map_err(io_error)?
                .status
                .success()
            {
                return Err("RUN_CONTAINER_REMOVE stage=worker_cleanup".into());
            }
        }
        Ok(())
    }
}
pub fn service_uid() -> Result<u32, String> {
    let output = Command::new("/usr/bin/id")
        .args(["-u", "sbxrun"])
        .output()
        .map_err(io_error)?;
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .map_err(|_| "RUN_SERVICE_ACCOUNT stage=worker_prepare".into())
}
fn docker(executor: &Executor) -> Command {
    let mut command = Command::new("/usr/bin/docker");
    command.args(&executor.args[..2]);
    command
}
fn io_error(error: std::io::Error) -> String {
    format!("RUN_RUNTIME_IO stage=worker_prepare: {error}")
}
