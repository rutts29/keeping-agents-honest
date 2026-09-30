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
    public_native: PathBuf,
    image: String,
    service_uid: u32,
    skills: Vec<(PathBuf, PathBuf)>,
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
        let domain = domain(record)?;
        let service_uid = service_uid()?;
        let mounts = kah_sandbox::publish_skills(
            home,
            &PathBuf::from(format!(
                "/var/tmp/kah-skills-{}",
                current_uid().map_err(|e| e.to_string())?
            )),
        )
        .map_err(|e| e.to_string())?;
        let executor = launch::sandbox(
            domain,
            &tree.path,
            &tree.git_dir,
            &public,
            image.trim(),
            service_uid,
            id,
        )?
        .with_read_only_mounts(&mounts)
        .map_err(|e| e.to_string())?;
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
            public_native: public,
            image: image.trim().into(),
            service_uid,
            skills: mounts,
        })
    }
    pub fn verification_executor(
        &self,
        domain: Domain,
        tree: &Worktree,
        id: &str,
    ) -> Result<Executor, String> {
        crate::access::share_tree(&tree.path, self.service_uid, true)?;
        launch::sandbox(
            domain,
            &tree.path,
            &tree.git_dir,
            &self.public_native,
            &self.image,
            self.service_uid,
            id,
        )?
        .with_read_only_mounts(&self.skills)
        .map_err(|e| e.to_string())
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
fn domain(record: &Record) -> Result<Domain, String> {
    Domain::ALL
        .into_iter()
        .find(|d| d.as_str() == record.domain)
        .ok_or_else(|| "RUN_DOMAIN stage=worker_prepare".into())
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
