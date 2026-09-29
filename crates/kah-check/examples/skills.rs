//! Gate 0: create a fresh ephemeral session and ask Codex to discover skills.
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

struct Probe {
    child: Child,
    lines: Receiver<String>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Probe {
    fn start() -> Self {
        let mut child = Command::new("codex")
            .args(["app-server", "--stdio"])
            .env_remove("HERDR_ENV")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("CODEX_START stage=skills");
        let stdout = child.stdout.take().unwrap();
        let (sender, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self { child, lines }
    }
    fn send(&mut self, value: Value) {
        writeln!(self.child.stdin.as_mut().unwrap(), "{value}").expect("CODEX_WRITE stage=skills");
    }
    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"id":id,"method":method,"params":params}));
        loop {
            let line = self
                .lines
                .recv_timeout(Duration::from_secs(20))
                .expect("CODEX_TIMEOUT stage=skills");
            let value: Value = serde_json::from_str(&line).expect("CODEX_JSON stage=skills");
            if value["id"] == id {
                assert!(
                    value.get("error").is_none(),
                    "CODEX_RPC stage=skills: {value}"
                );
                return value["result"].clone();
            }
        }
    }
}

fn main() {
    let mut probe = Probe::start();
    probe.request(
        1,
        "initialize",
        json!({"clientInfo":{"name":"kah_gate0","version":"0.1"}}),
    );
    probe.send(json!({"method":"initialized"}));
    let cwd = std::env::current_dir().unwrap();
    let session = probe.request(2, "thread/start", json!({"cwd":cwd,"ephemeral":true}));
    assert!(
        session["thread"]["id"].is_string(),
        "SESSION_START stage=skills"
    );
    let result = probe.request(3, "skills/list", json!({"cwds":[cwd],"forceReload":true}));
    let required = [
        "ponytail",
        "test-driven-development",
        "codebase-design",
        "incremental-implementation",
        "git-workflow-and-versioning",
        "code-simplification",
        "code-review-and-quality",
        "security-and-hardening",
        "diagnosing-bugs",
    ];
    let skills = result["data"][0]["skills"].as_array().unwrap();
    let selected: Vec<_> = required
        .iter()
        .map(|name| {
            let skill = skills
                .iter()
                .find(|skill| {
                    skill["name"]
                        .as_str()
                        .is_some_and(|found| found.rsplit(':').next() == Some(name))
                })
                .unwrap_or_else(|| panic!("SKILL_MISSING stage=skills: {name}"));
            assert_eq!(
                skill["enabled"], true,
                "SKILL_DISABLED stage=skills: {name}"
            );
            assert!(
                skill["path"]
                    .as_str()
                    .unwrap()
                    .contains("/agent-skills/sources/")
            );
            json!({"name":skill["name"],"path":skill["path"],"enabled":true})
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"freshProcess":true,
        "freshEphemeralSession":true,"skills":selected,"passed":true}))
        .unwrap()
    );
}
