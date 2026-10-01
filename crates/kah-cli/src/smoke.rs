//! Disposable native Codex command through the same Research executor as kah.
use crate::{Arguments, Launch, Result};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn run(home: &Path) -> Result<()> {
    let name = format!(
        "kah-smoke-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let project = home.join("repos/research").join(&name);
    if fs::symlink_metadata(&project).is_ok() {
        return Err("SMOKE_PROJECT_EXISTS stage=smoke".into());
    }
    let mut fixture = Fixture {
        project,
        overlay: None,
        canary: home.join(".cache").join(&name),
    };
    fs::File::create_new(&fixture.canary).map_err(|e| format!("SMOKE_CANARY stage=smoke: {e}"))?;
    let args = Arguments::parse(vec!["research".into(), name, "--no-observer".into()])?;
    let launch = Launch::prepare(home, home, args)?;
    fixture.overlay = Some(launch.overlay.clone());
    let script = format!(
        "test ! -e '{}' && test -d .git && printf 'KAH_SMOKE_OK\\n'",
        fixture.canary.to_string_lossy().replace('\'', "'\\''")
    );
    let prompt = format!(
        "Disposable executor health check. Use native exec_command exactly once with this shell command: {script}. No other tools, edits, installs, subagents or commits. Report and stop."
    );
    let mut bounded = bounded_native(&launch, &prompt);
    let output = bounded
        .output()
        .map_err(|e| format!("SMOKE_SPAWN stage=smoke: {e}"))?;
    if !output.status.success()
        || !command_passed(&output.stdout, &fixture.canary.to_string_lossy())
    {
        return Err(
            "SMOKE_NATIVE_COMMAND stage=smoke: native command did not prove executor isolation"
                .into(),
        );
    }
    let paths = [
        fixture.project.clone(),
        fixture.canary.clone(),
        launch.overlay.clone(),
    ];
    drop(fixture);
    if paths.iter().any(|p| p.exists()) {
        return Err("SMOKE_CLEANUP stage=smoke: disposable paths remain".into());
    }
    println!(
        "{}",
        json!({"passed":true,"nativeCommand":true,"hostCanaryHidden":true,"environment":launch.environment_id,"model":"native configured default","ephemeral":true,"disposablePathsRemoved":true,"observer":"disabled for health check; verified separately by interactive journeys"})
    );
    Ok(())
}
fn bounded_native(launch: &Launch, prompt: &str) -> Command {
    let mut native = launch.command(false);
    native.args(["exec", "--ephemeral", "--json", prompt]);
    let mut bounded = Command::new("/usr/bin/timeout");
    bounded
        .args(["--kill-after=5s", "150s"])
        .arg(native.get_program())
        .args(native.get_args())
        .current_dir(&launch.project.root)
        .stdin(Stdio::null());
    for (key, value) in native.get_envs() {
        if let Some(value) = value {
            bounded.env(key, value);
        } else {
            bounded.env_remove(key);
        }
    }
    bounded
        .env_remove("HERDR_ENV")
        .env_remove("CODEX_THREAD_ID");
    bounded
}
pub fn command_passed(bytes: &[u8], canary: &str) -> bool {
    bytes
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<Value>(line).ok())
        .any(|v| {
            v["type"] == "item.completed"
                && v["item"]["type"] == "command_execution"
                && v["item"]["exit_code"] == 0
                && v["item"]["command"].as_str().is_some_and(|c| {
                    c.contains("test ! -e") && c.contains(canary) && c.contains("test -d .git")
                })
                && v["item"]["aggregated_output"]
                    .as_str()
                    .is_some_and(|s| s.lines().any(|l| l == "KAH_SMOKE_OK"))
        })
}
struct Fixture {
    project: PathBuf,
    overlay: Option<PathBuf>,
    canary: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let socket =
            kah_run::access::service_uid().map(|id| format!("unix:///run/user/{id}/docker.sock"));
        if let Ok(socket) = socket {
            let ids = Command::new("/usr/bin/docker")
                .args([
                    "--host",
                    &socket,
                    "ps",
                    "-aq",
                    "--filter",
                    &format!("label=dev.kah.project={}", self.project.display()),
                ])
                .output();
            if let Ok(ids) = ids {
                for id in String::from_utf8_lossy(&ids.stdout).split_whitespace() {
                    let _ = Command::new("/usr/bin/docker")
                        .args(["--host", &socket, "rm", "-f", id])
                        .output();
                }
            }
        }
        let _ = fs::remove_dir_all(&self.project);
        if let Some(overlay) = &self.overlay {
            let _ = fs::remove_dir_all(overlay);
        }
        let _ = fs::remove_file(&self.canary);
    }
}
