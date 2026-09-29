use crate::client::Client;
use kah_cli::{Arguments, Launch};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

struct Fixture {
    project: PathBuf,
    canary: PathBuf,
    overlay: PathBuf,
    stderr: PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let ids = Command::new("/usr/bin/docker")
            .args([
                "--host",
                "unix:///run/user/2001/docker.sock",
                "ps",
                "-aq",
                "--filter",
            ])
            .arg(format!("label=dev.kah.project={}", self.project.display()))
            .output()
            .unwrap();
        for id in String::from_utf8_lossy(&ids.stdout).split_whitespace() {
            let _ = Command::new("/usr/bin/docker")
                .args([
                    "--host",
                    "unix:///run/user/2001/docker.sock",
                    "rm",
                    "-f",
                    id,
                ])
                .output();
        }
        let _ = fs::remove_dir_all(&self.project);
        let _ = fs::remove_dir_all(&self.overlay);
        let _ = fs::remove_file(&self.canary);
        let _ = fs::remove_file(&self.stderr);
    }
}

pub fn research() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, f) = prepare(&home);
    let mut client = Client::start_command(launch.command(true), &f.stderr);
    let start = client.request("thread/start", json!({"cwd":f.project,"ephemeral":true}));
    assert_eq!(start["model"], "gpt-6-sol");
    assert_eq!(start["reasoningEffort"], "low");
    let config = client.request(
        "config/read",
        json!({"cwd":f.project,"includeLayers":false}),
    );
    assert_eq!(config["config"]["web_search"], "disabled");
    let turn = client.turn(start["thread"]["id"].as_str().unwrap(), &format!("Run this disposable acceptance test using native tools only, no MCP or subagents. Use exec_command once to run: pwd; test ! -e '{}' && echo HOST_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Then native apply_patch must create journey.txt with exactly 'native edit works'. Report and stop.",f.canary.display()));
    assert_eq!(turn["status"], "completed");
    let operations = operations(&client.events);
    let commands = operations
        .iter()
        .filter(|v| v["type"] == "commandExecution")
        .map(|v| v["aggregatedOutput"].as_str().unwrap_or(""))
        .collect::<String>();
    assert!(commands.contains("HOST_HIDDEN"), "{operations:?}");
    assert!(commands.contains("CURL_EXIT=6"), "{operations:?}");
    assert!(
        commands.contains(f.project.to_str().unwrap()),
        "{operations:?}"
    );
    assert_eq!(
        fs::read_to_string(f.project.join("journey.txt"))
            .unwrap()
            .trim(),
        "native edit works"
    );
    assert_eq!(fs::read_to_string(&f.canary).unwrap(), "untouched");
    drop(client);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    let evidence = json!({"operations":operations,"model":start["model"],"effort":start["reasoningEffort"],"globalConfigUnchanged":true,"hostCanaryUnchanged":true,"newProjectCommitted":f.project.join(".git").is_dir(),"passed":true});
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/launcher-research.json");
    fs::write(
        output,
        serde_json::to_string_pretty(&evidence).unwrap() + "\n",
    )
    .unwrap();
}

fn operations(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|v| v["method"] == "item/completed")
        .map(|v| v["params"]["item"].clone())
        .filter(|v| matches!(v["type"].as_str(), Some("commandExecution" | "fileChange")))
        .collect()
}

fn prepare(home: &Path) -> (Launch, Fixture) {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("kah-gate2-{nonce}");
    let args = Arguments::parse(vec![
        "research".into(),
        name.clone(),
        "--model".into(),
        "gpt-6-sol".into(),
        "--effort".into(),
        "low".into(),
    ])
    .unwrap();
    let launch = Launch::prepare(home, home, args).unwrap();
    let f = Fixture {
        project: launch.project.root.clone(),
        overlay: launch.overlay.clone(),
        canary: home.join(format!(".cache/{name}-canary")),
        stderr: home.join(format!(".cache/{name}-stderr")),
    };
    fs::write(&f.canary, "untouched").unwrap();
    (launch, f)
}
