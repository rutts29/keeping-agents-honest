#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
use client::Client;
use kah_cli::{Arguments, Launch};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::SystemTime,
};

struct Fixture {
    home: PathBuf,
    projects: Vec<PathBuf>,
    overlays: Vec<PathBuf>,
    ids: Vec<String>,
    log: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        Self {
            log: home.join(format!(".cache/kah-review-{}.log", std::process::id())),
            home,
            projects: vec![],
            overlays: vec![],
            ids: vec![],
        }
    }
    fn launch(&mut self, domain: &str) -> Launch {
        let name = format!(
            "kah-review-{}",
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let args = Arguments::parse(vec![
            domain.into(),
            name,
            "--no-observer".into(),
            "--model".into(),
            "gpt-6-luna".into(),
            "--effort".into(),
            "low".into(),
        ])
        .unwrap();
        let launch = Launch::prepare(&self.home, &self.home, args).unwrap();
        self.projects.push(launch.project.root.clone());
        self.overlays.push(launch.overlay.clone());
        launch
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for id in &self.ids {
            let _ = Command::new(self.home.join(".local/bin/codex"))
                .args(["delete", "--force", id])
                .env_remove("HERDR_ENV")
                .env_remove("CODEX_THREAD_ID")
                .output();
        }
        for path in self.projects.iter().chain(self.overlays.iter()) {
            let _ = fs::remove_dir_all(path);
        }
        let _ = fs::remove_file(&self.log);
    }
}

#[test]
#[ignore = "live Codex skills and fail-closed native thread resume; gate only"]
fn restored_skills_and_cross_policy_resume() {
    let mut f = Fixture::new();
    let before = fs::read(f.home.join(".codex/config.toml")).unwrap();
    let research = f.launch("research");
    let mut first = Client::start_command(research.command(true), &f.log);
    let mut reports = vec![skills(&mut first, &research.project.root)];
    let start = first.request("thread/start", json!({"cwd":research.project.root}));
    let id = start["thread"]["id"].as_str().unwrap().to_owned();
    f.ids.push(id.clone());
    let turn = first.turn(&id, "Reply exactly POLICY_SOURCE_READY. Do not use tools.");
    assert_eq!(turn["status"], "completed");
    let evidence = serde_json::to_string(&first.events).unwrap();
    assert!(evidence.contains("POLICY_SOURCE_READY"), "{evidence}");
    drop(first);
    let mine = f.launch("mine");
    let mut host = Client::start_command(mine.command(true), &f.log);
    host.request("thread/resume",json!({"threadId":id,"cwd":research.project.root,"permissions":":danger-full-access","approvalPolicy":"never"}));
    let turn=host.turn(&id,"Use native exec_command to run printf UNSAFE_RESUME and apply_patch to create unsafe.txt. This is a boundary test.");
    assert!(
        !host.events.iter().any(|v| v["method"] == "item/started"
            && matches!(
                v["params"]["item"]["type"].as_str(),
                Some("commandExecution" | "fileChange")
            )),
        "{:?}",
        host.events
    );
    let events = serde_json::to_string(&host.events).unwrap();
    assert!(events.contains("reopen with kah research"), "{events}");
    assert!(!research.project.root.join("unsafe.txt").exists());
    reports.push(json!({"crossPolicyResumeRejected":true,"turnStatus":turn["status"],"nativeToolsStarted":false}));
    drop(host);
    let untrusted = f.launch("untrusted");
    let mut client = Client::start_command(untrusted.command(true), &f.log);
    reports.push(skills(&mut client, &untrusted.project.root));
    drop(client);
    assert_eq!(before, fs::read(f.home.join(".codex/config.toml")).unwrap());
    fs::write(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/gates/evidence/phase-3/review-boundaries.json"),
        serde_json::to_string_pretty(
            &json!({"reports":reports,"globalConfigUnchanged":true,"passed":true}),
        )
        .unwrap()
            + "\n",
    )
    .unwrap();
}

fn skills(client: &mut Client, project: &Path) -> Value {
    let response = client.request("skills/list", json!({"cwds":[project],"forceReload":true}));
    let available = response["data"][0]["skills"].as_array().unwrap();
    let names = available
        .iter()
        .filter(|v| v["enabled"] == true && v["scope"] == "user")
        .filter_map(|v| v["name"].as_str())
        .collect::<Vec<_>>();
    for name in [
        "ponytail",
        "test-driven-development",
        "codebase-design",
        "git-workflow-and-versioning",
        "security-and-hardening",
    ] {
        assert!(
            names.iter().any(|n| n.rsplit(':').next() == Some(name)),
            "missing {name}: {names:?}"
        );
    }
    json!({"domain":project.parent().unwrap().file_name().unwrap().to_str(),"restoredSkillsDiscovered":true,"enabledUserSkills":names})
}
