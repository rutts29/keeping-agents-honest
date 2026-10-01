use crate::{Arguments, Result, sandbox_command::Spec, setup_policy::SetupPolicy};
use kah_core::run_store::ensure_private_directory;
use kah_sandbox::{LaunchOptions, Project};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Setup {
    pub settings: Value,
    pub mcp: Option<PathBuf>,
    pub prefix: PathBuf,
    pub temporary: PathBuf,
}
pub fn prepare(home: &Path, project: &Project, args: &Arguments) -> Result<Setup> {
    let base = runtime_directory(project)?;
    let private = base.join(if args.options.online {
        "online"
    } else {
        "offline"
    });
    ensure_private_directory(&private)?;
    let temporary = private.join("tmp");
    ensure_private_directory(&temporary)?;
    let uid = kah_run::runtime::service_uid()?;
    let shared_root = base.parent().unwrap().parent().unwrap();
    for path in private
        .ancestors()
        .take_while(|p| p.starts_with(shared_root))
    {
        kah_run::access::traverse(path, uid)?;
    }
    kah_run::access::share_tree(&temporary, uid, true)?;
    kah_run::access::share_tree(&project.root, uid, true)?;
    let policy = SetupPolicy::load(home)?;
    let (mcp, environment) = mcp_configuration(home, &policy)?;
    write_specs(
        home,
        project,
        &private,
        &temporary,
        args.options.online,
        &environment,
        args.worktree_git.as_deref(),
    )?;
    let mcp = save_mcp(&private.join("control"), mcp)?;
    let prefix = private.join("prefix");
    let handler = home.join(".local/bin/kah");
    fs::write(
        &prefix,
        format!(
            "#!/bin/sh\nexec {} --sandbox-shell {} \"$1\"\n",
            quote(handler.to_str().ok_or("CLAUDE_PATH_UTF8 stage=setup")?),
            quote(private.join("control/executor.json").to_str().unwrap())
        ),
    )?;
    fs::set_permissions(&prefix, fs::Permissions::from_mode(0o700))?;
    let mut settings = json!({"hooks":{},"permissions":crate::claude_policy::permissions(home,&project.root,&private,args.options.online,args.worktree_git.as_deref())?});
    settings["deniedMcpServers"] = json!(
        policy
            .secret_mcp_servers
            .iter()
            .filter(|(_, enabled)| !**enabled)
            .map(|(name, _)| json!({"serverName":name}))
            .collect::<Vec<_>>()
    );
    settings["env"] = json!({"CLAUDE_CODE_SHELL_PREFIX":prefix,"CLAUDE_CODE_TMPDIR":temporary});
    Ok(Setup {
        settings,
        mcp,
        prefix,
        temporary,
    })
}

pub fn runtime_directory(project: &Project) -> Result<PathBuf> {
    let hash = format!(
        "{:x}",
        Sha256::digest(project.root.as_os_str().as_encoded_bytes())
    );
    Ok(PathBuf::from(format!(
        "/var/tmp/kah-claude-runtime-{}",
        kah_sandbox::current_uid()?
    ))
    .join(project.domain.as_str())
    .join(&hash[..16]))
}
impl Setup {
    pub fn apply(&self, command: &mut Command) {
        command
            .args(["--setting-sources", "user"])
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("CLAUDE_CODE_SHELL_PREFIX", &self.prefix)
            .env("CLAUDE_CODE_TMPDIR", &self.temporary)
            .env_remove("KAH_SANDBOX_PROFILE");
        if let Some(path) = &self.mcp {
            command.arg("--mcp-config").arg(path);
        }
    }
}
pub fn mcp_configuration(home: &Path, policy: &SetupPolicy) -> Result<(Value, Vec<String>)> {
    let original = read_json(&home.join(".claude.json"))?;
    let mut servers = original
        .get("mcpServers")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let map = servers
        .as_object_mut()
        .ok_or("CLAUDE_MCP_TYPE stage=setup")?;
    map.retain(|name, _| policy.secret_mcp_servers.get(name) != Some(&false));
    let mut environment = std::collections::BTreeSet::new();
    for (name, server) in map {
        if server.get("command").is_none() {
            continue;
        }
        let env = server
            .as_object_mut()
            .ok_or("CLAUDE_MCP_TYPE stage=setup")?
            .entry("env")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("CLAUDE_MCP_ENV stage=setup")?;
        environment.extend(env.keys().cloned());
        env.insert(
            "KAH_SANDBOX_PROFILE".into(),
            if policy.offline_mcp_servers.contains(name) {
                "mcp-offline"
            } else {
                "mcp-online"
            }
            .into(),
        );
    }
    Ok((servers, environment.into_iter().collect()))
}
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

fn write_specs(
    home: &Path,
    project: &Project,
    private: &Path,
    temporary: &Path,
    online: bool,
    environment: &[String],
    worktree_git: Option<&Path>,
) -> Result<()> {
    let uid = kah_run::runtime::service_uid()?;
    let base =
        crate::native_setup::executor(home, project, LaunchOptions::default(), worktree_git)?;
    for (name, online) in [
        ("executor", online),
        ("mcp-online", true),
        ("mcp-offline", false),
    ] {
        let mut executor = base.clone();
        executor.set_network(online)?;
        executor.use_nobody();
        executor.add_writable_mount(temporary, temporary)?;
        Spec {
            executor,
            temporary: Some(temporary.to_owned()),
            service_uid: uid,
            environment: if name == "executor" {
                vec![]
            } else {
                environment.to_vec()
            },
        }
        .save(&private.join("control").join(format!("{name}.json")))?;
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "CLAUDE_SETTINGS_JSON stage=setup: invalid JSON".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(format!("CLAUDE_SETUP_IO stage=setup: {e}").into()),
    }
}

fn save_mcp(directory: &Path, servers: Value) -> Result<Option<PathBuf>> {
    if servers.as_object().is_none_or(|m| m.is_empty()) {
        return Ok(None);
    }
    crate::unattended_report::save(directory, "mcp.json", &json!({"mcpServers":servers}))
        .map_err(|e| format!("CLAUDE_MCP_WRITE stage=setup: {e}"))?;
    Ok(Some(directory.join("mcp.json")))
}
