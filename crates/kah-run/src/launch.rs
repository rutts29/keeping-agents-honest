//! Native non-interactive agent; credentials and inference remain on the host.
use kah_sandbox::{Domain, Executor, LaunchOptions, Project};
use std::{path::Path, process::Command};

pub fn sandbox(
    domain: Domain,
    worktree: &Path,
    git_dir: &Path,
    public_codex: &Path,
    image: &str,
    service_uid: u32,
    id: &str,
) -> Result<Executor, String> {
    let project = Project {
        domain,
        root: worktree.into(),
        relative_cwd: Default::default(),
        needs_creation: false,
    };
    let mounts = vec![
        (git_dir.into(), git_dir.into()),
        (worktree.join(".git"), worktree.join(".git")),
    ];
    let mut executor = Executor::new(
        &project,
        LaunchOptions::default(),
        public_codex,
        image,
        service_uid,
    )
    .and_then(|e| e.with_read_only_mounts(&mounts))
    .map_err(|e| e.to_string())?;
    for arg in &mut executor.args {
        if arg == "0:0" {
            *arg = "65534:65534".into();
        } else if arg == "/home/sandbox:rw,nosuid,nodev,size=512m,mode=700" {
            arg.push_str(",uid=65534,gid=65534");
        }
    }
    executor.insert_options([
        "--label".into(),
        format!("dev.kah.run={id}"),
        "--env".into(),
        "GIT_OPTIONAL_LOCKS=0".into(),
        "--env".into(),
        "GIT_CONFIG_NOSYSTEM=1".into(),
        "--env".into(),
        "GIT_CONFIG_GLOBAL=/dev/null".into(),
    ]);
    // gVisor maps operator-owned files to nobody. Use that unprivileged
    // identity, and preserve group write access for the host's inherited ACLs.
    let at = executor.args.len() - 4;
    executor.args.splice(
        at..at,
        [
            "/bin/sh".into(),
            "-c".into(),
            "umask 0002; exec \"$@\"".into(),
            "kah-executor".into(),
        ],
    );
    Ok(executor)
}

pub struct WorkerLaunch<'a> {
    pub native: &'a Path,
    pub overlay: &'a Path,
    pub worktree: &'a Path,
    pub model: Option<&'a str>,
    pub effort: Option<&'a str>,
}
impl WorkerLaunch<'_> {
    pub fn command(&self) -> Command {
        let mut command = parent_bound(self.native);
        command
            .args([
                "exec",
                "--ephemeral",
                "--json",
                "--skip-git-repo-check",
                "--ignore-rules",
                "--sandbox",
                "danger-full-access",
                "-C",
            ])
            .arg(self.worktree)
            .current_dir(self.worktree)
            .env("CODEX_HOME", self.overlay)
            .env_remove("CODEX_EXEC_SERVER_URL")
            .env_remove("CODEX_THREAD_ID")
            .env_remove("HERDR_ENV")
            .env_remove("KAH_OBSERVER_BINDING")
            .env_remove("KAH_SESSION_POLICY");
        if let Some(model) = self.model {
            command.args(["--model", model]);
        }
        if let Some(effort) = self.effort {
            command.args([
                "-c",
                &format!(
                    "model_reasoning_effort={}",
                    serde_json::to_string(effort).unwrap()
                ),
            ]);
        }
        // Observer intake uses this process's JSON events: ephemeral threads
        // have no durable rollout for lifecycle hooks to read.
        for setting in [
            "approval_policy=\"never\"",
            "web_search=\"disabled\"",
            "features.hooks=false",
            "features.apps=false",
            "features.plugins=false",
            "features.remote_plugin=false",
            "features.multi_agent=false",
            "features.multi_agent_v2=false",
            "features.goals=false",
        ] {
            command.args(["-c", setting]);
        }
        for variable in [
            "OPENAI_API_KEY",
            "OPENROUTER_API_KEY",
            "ANTHROPIC_API_KEY",
            "TYPESAFE_API_KEY",
            "CODEX_ACCESS_TOKEN",
        ] {
            command.env_remove(variable);
        }
        command.arg("-");
        command
    }
}

fn parent_bound(native: &Path) -> Command {
    // Kernel-enforced parent death, without a privileged supervisor or a PID
    // recovery race. Check PPID after installing the signal: the controller
    // could have exited before setpriv configured it.
    let mut command = Command::new("/usr/bin/setpriv");
    command
        .args([
            "--pdeathsig",
            "KILL",
            "/bin/sh",
            "-c",
            "test \"$PPID\" -eq \"$1\" || exit 125; shift; exec \"$@\"",
            "kah-worker",
        ])
        .arg(std::process::id().to_string())
        .arg(native);
    command
}
