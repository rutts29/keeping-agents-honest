#[allow(dead_code)]
mod claude_support;
#[path = "journeys/fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "terminal_support/history.rs"]
mod history;
#[allow(dead_code)]
#[path = "terminal_support/processes.rs"]
mod processes;
#[allow(dead_code)]
#[path = "terminal_support/pty.rs"]
mod pty;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

#[test]
#[ignore = "real Claude first-prompt native title, resume and explicit name"]
fn claude_interactive_names() {
    for explicit in [false, true] {
        let f = claude_support::Fixture::new();
        let extra = if explicit {
            vec!["--name", "My own session"]
        } else {
            vec![]
        };
        let mut terminal = f.start_with(false, "haiku", "low", &extra);
        f.ready(&mut terminal);
        let task = "steer: Reply NAMING_OK. No tools, edits or subagents.";
        f.prompt(&mut terminal, task);
        f.settled(&mut terminal, task);
        let expected = if explicit {
            "My own session".into()
        } else {
            kah_run::native::session_name(
                &format!(
                    "{}-{}",
                    f.project().file_name().unwrap().to_str().unwrap(),
                    f.id
                ),
                task,
            )
        };
        f.wait(&mut terminal, || {
            f.rows()
                .iter()
                .any(|r| r["type"] == "custom-title" && r["customTitle"] == expected)
        });
        drop(terminal);
        let mut resumed = f.start(true);
        f.ready(&mut resumed);
        let next = "steer: Reply STILL_NAMED. No tools or edits.";
        f.prompt(&mut resumed, next);
        f.settled(&mut resumed, next);
        let rows = f.rows();
        let last = rows
            .iter()
            .rev()
            .find(|r| r["type"] == "custom-title")
            .unwrap();
        assert_eq!(last["customTitle"], expected);
        drop(resumed);
        save(
            &format!(
                "claude-{}.json",
                if explicit { "explicit" } else { "automatic" }
            ),
            "claude",
            json!({"nativeTitle":expected,"firstPromptNaming":!explicit,"explicitNamePreserved":explicit,"resumePreservesTitle":true,"fixtureModel":"haiku","fixtureEffort":"low"}),
        );
    }
}

#[test]
#[ignore = "real Codex TUI first-turn native API name and user rename preservation"]
fn codex_interactive_names() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut f) = fixture::prepare(&home, "research", false);
    let mut command = launch.command(false);
    command.args([
        "--no-alt-screen",
        "steer: Reply NAMING_OK. No tools, edits or subagents.",
    ]);
    let mut terminal = pty::Terminal::start(&pty::command_line(&command), &f.stderr);
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    let expected = kah_run::native::session_name(
        &format!("{}-{id}", f.project.file_name().unwrap().to_str().unwrap()),
        "steer: Reply NAMING_OK. No tools, edits or subagents.",
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let read = reader
            .request("thread/read", json!({"threadId":id,"includeTurns":false}))
            .unwrap();
        if read["thread"]["name"] == expected {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native title absent: {}",
            read["thread"]["name"]
        );
        std::thread::sleep(Duration::from_millis(300));
    }
    reader
        .request(
            "thread/name/set",
            json!({"threadId":id,"name":"My own session"}),
        )
        .unwrap();
    terminal.submit("steer: Reply STILL_NAMED. No tools or edits.");
    history::completed_turn(&mut reader, &id, turn["id"].as_str());
    drop(terminal);
    let read = reader
        .request("thread/read", json!({"threadId":id,"includeTurns":false}))
        .unwrap();
    assert_eq!(read["thread"]["name"], "My own session");
    drop(reader);
    drop(f);
    assert_eq!(before, fs::read(home.join(".codex/config.toml")).unwrap());
    save(
        "codex.json",
        "codex",
        json!({"nativeTitle":expected,"firstTurnNaming":true,"userRenamePreserved":true,"fixtureModel":"gpt-6-sol","fixtureEffort":"low","globalConfigUnchanged":true}),
    );
}
fn save(file: &str, agent: &str, mut value: Value) {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let version = Command::new(home.join(".local/bin").join(agent))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    value["version"] = String::from_utf8(version.stdout).unwrap().trim().into();
    value["passed"] = true.into();
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        evidence.join(file),
        serde_json::to_vec_pretty(&value).unwrap(),
    )
    .unwrap();
}
