//! Native fixture only; public evidence contains shapes, version and assertions.
use serde_json::{Value, json};
use std::{fs, path::PathBuf, process::Command};

#[test]
#[ignore = "real Claude login, transcript and hooks; opt-in live run"]
fn installed_transcript_and_hooks() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let project = home.join(format!(
        "repos/mine/kah-claude-probe-{}",
        std::process::id()
    ));
    fs::create_dir(&project).unwrap();
    let hooks = project.join("hooks.jsonl");
    let command = format!(
        "cat >> '{}'; printf '\\n' >> '{}'",
        hooks.display(),
        hooks.display()
    );
    let settings = json!({"hooks":{
        "SessionStart":[{"hooks":[{"type":"command","command":command}]}],
        "UserPromptSubmit":[{"hooks":[{"type":"command","command":command}]}],
        "Stop":[{"hooks":[{"type":"command","command":command}]}]
    }});
    let version = Command::new("claude").arg("--version").output().unwrap();
    assert!(version.status.success());
    let reply = run_native(&project, &settings);
    let events: Vec<Value> = fs::read_to_string(&hooks)
        .unwrap()
        .lines()
        .filter(|s| !s.trim().is_empty())
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let transcript = PathBuf::from(events[0]["transcript_path"].as_str().unwrap());
    let source = fs::read_to_string(&transcript).unwrap();
    let snapshot =
        kah_claude::transcript::parse(&source, reply["session_id"].as_str().unwrap(), &project)
            .unwrap();
    assert_eq!(
        snapshot.users.unwrap().authoritative_records()[0].content(),
        "Reply exactly KAH_CLAUDE_PROBE."
    );
    let report = json!({"version":String::from_utf8(version.stdout).unwrap().trim(),"passed":true,
        "modelArgsSupplied":false,"effortArgsSupplied":false,"modelUsage":reply["modelUsage"],
        "hookFields":events.iter().map(|e| json!({"event":e["hook_event_name"],"fields":e.as_object().unwrap().keys().collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "transcriptShapes":source.lines().filter_map(|s| serde_json::from_str::<Value>(s).ok()).map(|r| json!({"type":r["type"],"origin":r["origin"],"isMeta":r["isMeta"],"fields":r.as_object().unwrap().keys().collect::<Vec<_>>()})).collect::<Vec<_>>()});
    let evidence = PathBuf::from(std::env::var_os("KAH_GATE_EVIDENCE").unwrap());
    fs::write(
        evidence.join("native-hooks-transcript.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    fs::remove_dir_all(&project).unwrap();
    if transcript
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .contains("kah-claude-probe-")
    {
        fs::remove_dir_all(transcript.parent().unwrap()).unwrap();
    }
}

fn run_native(project: &std::path::Path, settings: &Value) -> Value {
    let output = Command::new("timeout")
        .args([
            "120",
            "claude",
            "-p",
            "--output-format",
            "json",
            "--tools",
            "",
            "--setting-sources",
            "user",
            "--settings",
        ])
        .arg(settings.to_string())
        .arg("Reply exactly KAH_CLAUDE_PROBE.")
        .current_dir(project)
        .env_remove("HERDR_ENV")
        .env_remove("CLAUDECODE")
        .env_remove("CODEX_THREAD_ID")
        .env_remove("KAH_OBSERVER_BINDING")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Claude probe exit {:?}: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
