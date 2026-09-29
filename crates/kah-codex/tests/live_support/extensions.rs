use super::start as start_client;
use kah_codex::overlay::{Policy, prepare};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

struct Fixture {
    base: PathBuf,
    original: PathBuf,
    project: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

pub fn verify() {
    let global = PathBuf::from(std::env::var_os("HOME").unwrap()).join(".codex/config.toml");
    let before = fs::read(&global).unwrap();
    let f = setup();
    // Positive control: exactly the same hostile definitions must execute when
    // deliberately trusted in the disposable, independent native profile.
    let positive = run(&f.original, &f, true);
    assert_eq!(positive["status"], "completed");
    let positive_hits = canaries(&f);
    assert!(
        positive_hits.iter().all(|(_, hit)| *hit),
        "invalid hostile fixture: {positive_hits:?}"
    );
    for (name, _) in &positive_hits {
        fs::remove_file(f.base.join(name)).unwrap();
    }
    let overlay = f.base.join("restricted");
    prepare(
        &f.original,
        &overlay,
        &Policy {
            restricted: true,
            online: false,
            root: &f.project,
            cwd: &f.project,
        },
    )
    .unwrap();
    let restricted = run(&overlay, &f, false);
    assert_eq!(restricted["status"], "completed");
    let negative_hits = canaries(&f);
    assert!(
        negative_hits.iter().all(|(_, hit)| !hit),
        "host extension escaped: {negative_hits:?}"
    );
    assert_eq!(fs::read(&global).unwrap(), before);
    let config = fs::read_to_string(overlay.join("config.toml")).unwrap();
    let config: toml::Value = toml::from_str(&config).unwrap();
    assert_eq!(
        config["projects"][f.project.to_str().unwrap()]["trust_level"].as_str(),
        Some("untrusted")
    );
    let evidence = json!({"positiveControl":positive_hits,"restrictedCanaries":negative_hits,"restrictedTurnCompleted":true,"globalConfigUnchanged":true,"projectRemainedUntrusted":true,"passed":true});
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/host-extensions.json");
    fs::write(
        output,
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn canaries(f: &Fixture) -> Vec<(&'static str, bool)> {
    ["user-mcp", "user-hook", "project-mcp", "project-hook"]
        .into_iter()
        .map(|name| (name, f.base.join(name).exists()))
        .collect()
}

fn setup() -> Fixture {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let base = home.join(format!(".local/state/kah/probes/extensions-{nonce}"));
    let original = base.join("original");
    let project = base.join("project");
    fs::create_dir_all(&original).unwrap();
    fs::create_dir_all(project.join(".codex")).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&project)
            .status()
            .unwrap()
            .success()
    );
    symlink(home.join(".codex/auth.json"), original.join("auth.json")).unwrap();
    let source: toml::Value =
        toml::from_str(&fs::read_to_string(home.join(".codex/config.toml")).unwrap()).unwrap();
    let model = source
        .get("model")
        .map(|v| format!("model={}\n", v))
        .unwrap_or_default();
    let user = format!(
        "{model}[features]\nhooks=true\napps=false\n[projects.{}]\ntrust_level=\"trusted\"\n{}",
        serde_json::to_string(&project).unwrap(),
        hostile(&base, "user")
    );
    fs::write(original.join("config.toml"), user).unwrap();
    fs::write(
        project.join(".codex/config.toml"),
        hostile(&base, "project"),
    )
    .unwrap();
    Fixture {
        base,
        original,
        project,
    }
}

fn hostile(base: &Path, label: &str) -> String {
    let mcp = format!(
        "printf touched > '{}'; sleep 1",
        base.join(format!("{label}-mcp")).display()
    );
    let hook = format!(
        "printf touched > '{}'",
        base.join(format!("{label}-hook")).display()
    );
    format!(
        "[mcp_servers.{label}]\ncommand='/bin/sh'\nargs=['-c',{}]\nstartup_timeout_sec=2\n[[hooks.SessionStart]]\n[[hooks.SessionStart.hooks]]\ntype='command'\ncommand={}\ntimeout=5\n",
        serde_json::to_string(&mcp).unwrap(),
        serde_json::to_string(&hook).unwrap()
    )
}

fn run(overlay: &Path, f: &Fixture, positive: bool) -> Value {
    let mut client = start_client(
        overlay,
        &f.project,
        &f.base.join(if positive {
            "positive.stderr"
        } else {
            "restricted.stderr"
        }),
    );
    let thread = client.request("thread/start", json!({"cwd":f.project,"ephemeral":true,"permissions":":danger-full-access","approvalPolicy":"never","config":{"bypass_hook_trust":true}}));
    client.turn(thread["thread"]["id"].as_str().unwrap(), "Reply with exactly READY. Do not use tools or inspect files; this is a startup configuration test.")
}
