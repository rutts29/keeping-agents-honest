use crate::{Result, unattended_args::RunArguments, unattended_observer::Settings};
use kah_core::{run_store::RunStore, unattended::RunControl};
use kah_observer::{judge::routing::Backend, session::Binding};
use kah_run::{
    access,
    access::service_uid,
    store::{Record, Store},
    worktree::Worktree,
};
use kah_sandbox::{Project, current_uid};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn create(home: &Path, cwd: &Path, args: RunArguments) -> Result<String> {
    let mut control = RunControl::new(&args.task, args.limits)?;
    let observer = kah_observer::compaction::CompactorConfig::new(
        args.observer_model.as_deref().unwrap_or("gpt-6-luna"),
        Some(args.observer_effort.as_deref().unwrap_or("low")),
    )?;
    let project = Project::select(&home.join("repos"), cwd, None)?;
    let domain = args.domain.unwrap_or(project.domain);
    let id = format!(
        "run-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("RUN_CLOCK stage=run_create: {e}"))?
            .as_nanos(),
        std::process::id()
    );
    let base = RunStore::default_state_root()?.join("runs");
    let work_root = workspace_root()?;
    let tree = Worktree::create(&project.root, &work_root.join(&id), &id)?;
    access::traverse(tree.path.parent().unwrap(), service_uid()?)?;
    access::share_tree(&tree.path, service_uid()?, true)?;
    access::share_tree(&tree.git_dir, service_uid()?, false)?;
    control.checkpoint(tree.base_commit.clone())?;
    let mut native = kah_run::native::Native::new(args.agent.as_deref().unwrap_or("codex"))?;
    native.online = args.online;
    native.name = Some(kah_run::native::session_name(&id, control.task()));
    let record = Record {
        native: Some(native),
        project: project.root,
        domain: domain.as_str().into(),
        worktree: tree.path.clone(),
        model: args.model,
        effort: args.effort,
        control,
    };
    let store = Store::create(&base, &id, record)?;
    tree.save(&store.directory().join("worktree.json"))?;
    let settings = Settings {
        test: args.test,
        allow_hosts: vec![],
        observer: Binding {
            session: id.clone(),
            project: tree.path.clone(),
            codex_home: home.join(".codex"),
            native: home
                .join(".local/bin/codex")
                .canonicalize()
                .map_err(|e| format!("RUN_NATIVE_BINARY stage=run_create: {e}"))?,
            model: observer.model().into(),
            effort: observer.reasoning().into(),
            timeout_secs: 45,
            judge: Backend::Jev,
            jev_model: args.jev_model.unwrap_or_else(|| "jev-latest".into()),
        },
    };
    save(&store.directory().join("settings.json"), &settings)?;
    Ok(id)
}

pub fn settings(directory: &Path) -> Result<Settings> {
    let path = directory.join("settings.json");
    if !fs::symlink_metadata(&path).map_err(settings_io)?.is_file() {
        return Err("RUN_SETTINGS_TYPE stage=resume".into());
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(settings_io)?
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(settings_io)?;
    if bytes.len() > 128 * 1024 {
        return Err("RUN_SETTINGS_BOUND stage=resume".into());
    }
    Ok(serde_json::from_slice(&bytes)
        .map_err(|e| format!("RUN_SETTINGS_JSON stage=resume: {e}"))?)
}
fn workspace_root() -> Result<PathBuf> {
    let uid = current_uid()?;
    let path = PathBuf::from(format!("/var/tmp/kah-runs-{uid}"));
    match fs::DirBuilder::new().mode(0o700).create(&path) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(format!("RUN_WORKSPACE_CREATE stage=run_create: {e}").into()),
    }
    let metadata = fs::symlink_metadata(&path)
        .map_err(|e| format!("RUN_WORKSPACE_METADATA stage=run_create: {e}"))?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.mode() & 0o022 != 0 {
        return Err(
            "RUN_WORKSPACE_ROOT stage=run_create: expected private operator-owned directory".into(),
        );
    }
    access::traverse(&path, service_uid()?)?;
    Ok(path)
}
fn save(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(settings_io)?;
    file.write_all(
        &serde_json::to_vec_pretty(value)
            .map_err(|e| format!("RUN_SETTINGS_JSON stage=run_create: {e}"))?,
    )
    .map_err(settings_io)?;
    file.sync_all().map_err(settings_io)?;
    Ok(())
}

fn settings_io(error: std::io::Error) -> String {
    format!("RUN_SETTINGS_IO stage=run_settings: {error}")
}
