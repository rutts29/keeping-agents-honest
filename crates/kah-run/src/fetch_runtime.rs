use crate::{
    access,
    fetch_exchange::Broker,
    fetch_policy::FetchPolicy,
    runtime::{Runtime, service_uid},
    store::Record,
};
use kah_codex::overlay;
use kah_sandbox::{current_uid, publish_binary};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
};

pub fn enable(
    runtime: &mut Runtime,
    record: &Record,
    hosts: &[String],
) -> Result<Option<Broker>, String> {
    let policy = FetchPolicy::new(&record.domain, hosts.to_vec())?;
    if hosts.is_empty() {
        return Ok(None);
    }
    let directory = record
        .worktree
        .parent()
        .ok_or("FETCH_WORKTREE stage=fetch_setup")?
        .join("fetch-exchange");
    match fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => access::share_tree(&directory, service_uid()?, true)?,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(format!("FETCH_DIRECTORY stage=fetch_setup: {e}")),
    }
    let uid = current_uid().map_err(|e| e.to_string())?;
    let meta = fs::symlink_metadata(&directory)
        .map_err(|e| format!("FETCH_DIRECTORY stage=fetch_setup: {e}"))?;
    if !meta.is_dir() || meta.uid() != uid {
        return Err("FETCH_DIRECTORY_OWNER stage=fetch_setup".into());
    }
    let source =
        std::env::current_exe().map_err(|e| format!("FETCH_HELPER stage=fetch_setup: {e}"))?;
    let binary = publish_binary(
        &source,
        &PathBuf::from(format!("/var/tmp/kah-fetch-runtime-{uid}")),
    )
    .map_err(|e| e.to_string())?;
    runtime.executor = mount(runtime.executor.clone(), &directory, &binary)?;
    runtime.fetch_mounts = Some((directory.clone(), binary));
    overlay::set_executor(&runtime.overlay, "kah-unattended", &runtime.executor.args)
        .map_err(|e| e.to_string())?;
    Ok(Some(Broker::start(directory, policy)?))
}
pub fn mount(
    executor: kah_sandbox::Executor,
    directory: &Path,
    binary: &Path,
) -> Result<kah_sandbox::Executor, String> {
    let mut executor = executor
        .with_read_only_mounts(&[(binary.into(), PathBuf::from("/opt/kah"))])
        .map_err(|e| e.to_string())?;
    executor
        .add_writable_mount(directory, Path::new("/run/kah-fetch"))
        .map_err(|e| e.to_string())?;
    executor.insert_options(["--env".into(), "KAH_FETCH_DIR=/run/kah-fetch".into()]);
    Ok(executor)
}
