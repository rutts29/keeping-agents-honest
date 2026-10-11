//! Share installed setup directly and use native configuration for execution.
use crate::{Result, sandbox_command::Spec, setup_policy::SetupPolicy};
use kah_codex::overlay;
use kah_sandbox::{Executor, LaunchOptions, Project, current_uid, publish_binary};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
};

pub fn executor(
    home: &Path,
    project: &Project,
    options: LaunchOptions,
    worktree_git: Option<&Path>,
) -> Result<Executor> {
    let uid = kah_run::access::service_uid()?;
    let binary = publish_binary(
        &home.join(".local/bin/codex"),
        &PathBuf::from(format!("/var/tmp/kah-runtime-{}", current_uid()?)),
    )?;
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref")
        .map_err(|e| format!("SANDBOX_IMAGE_READ stage=setup: {e}"))?;
    let mut mounts =
        kah_sandbox::global_setup_mounts_with_claude(home, &kah_claude::paths::config_root(home))?;
    for (source, target) in mounts.clone() {
        share(&source, uid)?;
        if let Ok(relative) = target.strip_prefix(home) {
            mounts.push((source, Path::new("/home/sandbox").join(relative)));
        }
    }
    mounts.retain(|(_, target)| !target.starts_with(&project.root));
    let mut executor = Executor::new(project, options, &binary, image.trim(), uid)?
        .with_read_only_mounts(&mounts)?;
    if let Some(git) = worktree_git {
        executor.use_nobody();
        kah_run::access::share_tree(git, uid, true)?;
        executor.add_writable_mount(git, git)?;
    }
    Ok(kah_sandbox::git_guard::protect(
        executor,
        &project.root,
        worktree_git,
    )?)
}

pub fn codex(
    home: &Path,
    project: &Project,
    directory: &Path,
    policy_name: &str,
    options: LaunchOptions,
    worktree_git: Option<&Path>,
) -> Result<String> {
    let offline = executor(home, project, options, worktree_git)?;
    let mut online = offline.clone();
    online.set_network(true)?;
    let mut mcp_offline = offline.clone();
    mcp_offline.set_network(false)?;
    let id = format!("kah-{policy_name}");
    overlay::set_executors(
        directory,
        &id,
        &[
            (&id, &offline.args),
            ("kah-mcp-online", &online.args),
            ("kah-mcp-offline", &mcp_offline.args),
        ],
    )?;
    let policy = SetupPolicy::load(home)?;
    overlay::route_mcp(
        directory,
        &project.root,
        "kah-mcp-offline",
        "kah-mcp-online",
        &policy.offline_mcp_servers,
        &policy.secret_mcp_servers,
    )?;
    let spec = directory.join("control/hooks.json");
    Spec {
        executor: offline,
        temporary: None,
        service_uid: kah_run::access::service_uid()?,
        environment: vec![],
    }
    .save(&spec)?;
    overlay::write_settings(
        directory,
        serde_json::json!({"features":{"hooks":true},"web_search":"live"}),
    )?;
    overlay::sandbox_hooks(
        &home.join(".local/bin/codex"),
        directory,
        &project.root,
        &[
            home.join(".local/bin/kah").to_string_lossy().into_owned(),
            "--sandbox-shell".into(),
            spec.to_string_lossy().into_owned(),
        ],
    )?;
    Ok(id)
}

fn share(path: &Path, uid: u32) -> Result<()> {
    let operator = current_uid()?;
    // Config and installed aliases can live outside HOME. Traverse owned
    // ancestors only; never list a config root or modify root-owned directories.
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent)
            .map_err(|e| format!("SETUP_PARENT_METADATA stage=setup: {e}"))?;
        if metadata.uid() != operator {
            break;
        }
        kah_run::access::traverse(parent, uid)?;
    }
    if path.is_dir() {
        kah_run::access::share_tree(path, uid, false)?;
    } else {
        let output = Command::new("/usr/bin/setfacl")
            .args(["-P", "-m", &format!("u:{uid}:r"), "--"])
            .arg(path)
            .output()?;
        if !output.status.success() {
            return Err("SETUP_FILE_ACCESS stage=setup: cannot share installed setup file".into());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/native_setup_access/mod.rs"]
mod tests;
