use super::{fixture::prepare, operations};
use crate::client::Client;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub fn resume_and_loss() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut f) = prepare(&home, "research", false);
    let mut first = Client::start_command(launch.command(true), &f.stderr);
    let start = first.request("thread/start", json!({"cwd":f.project}));
    let id = start["thread"]["id"].as_str().unwrap();
    f.sessions.push(id.into());
    assert_eq!(start["model"], "gpt-6.1-sol");
    let turn = first.turn_with(id,"Native acceptance check: use exec_command once to print MODEL_SWITCH_WORKS, then native apply_patch to create resume-proof.txt containing PRESERVED. No other tools, no subagents, no commits. Report and stop.",Some("gpt-6-astra"),Some("low"));
    assert_eq!(turn["status"], "completed");
    assert_eq!(
        fs::read_to_string(f.project.join("resume-proof.txt"))
            .unwrap()
            .trim(),
        "PRESERVED"
    );
    drop(first);
    let mut resumed = Client::start_command(launch.command(true), &f.stderr);
    let resume = resumed.request("thread/resume",json!({"threadId":id,"cwd":f.project,"permissions":":danger-full-access","approvalPolicy":"never"}));
    assert_eq!(resume["model"], "gpt-6-astra");
    assert_eq!(resume["reasoningEffort"], "low");
    let turn = resumed.turn(id,&format!("Use native exec_command once to run: test -f resume-proof.txt && echo EDIT_PRESERVED; test ! -e '{}' && echo HOST_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Report and stop, no other tools.",f.canary.display()));
    assert_eq!(turn["status"], "completed");
    let resume_operations = operations(&resumed.events);
    let text = serde_json::to_string(&resume_operations).unwrap();
    for marker in ["EDIT_PRESERVED", "HOST_HIDDEN", "CURL_EXIT=6"] {
        assert!(text.contains(marker), "{text}");
    }
    let killed = kill_executor(&f.project);
    let at = resumed.events.len();
    let loss = resumed.turn(id,&format!("An authorized test driver killed your exec-server. Attempt each native operation exactly once, even if the first fails. First exec_command with workdir /tmp and command: printf LOCAL_FALLBACK > '{}'. Then native apply_patch updating '{}' from untouched to LOCAL_FALLBACK. Both are disposable host canaries. No repair, escalation, MCP, subagents, retries, or alternatives. Report both failures and stop.",f.canary.display(),f.canary.display()));
    assert_eq!(loss["status"], "completed");
    let loss_operations = operations(&resumed.events[at..]);
    drop(resumed);
    let errors = fs::read_to_string(&f.stderr).unwrap();
    assert!(
        errors.contains("exec-server transport disconnected"),
        "{errors}"
    );
    assert!(
        errors.contains("apply_patch") && errors.contains("exec-server transport closed"),
        "{errors}"
    );
    assert_eq!(fs::read_to_string(&f.canary).unwrap(), "untouched");
    let report = json!({"initialModel":start["model"],"resumedModel":resume["model"],"resumedEffort":resume["reasoningEffort"],"resumeOperations":resume_operations,"lossOperations":loss_operations,"nativeErrors":errors,"killedProjectExecutors":killed,"executorLossScope":"all containers belonging to this exact fixture project, including MCP executors","hostCanaryUnchanged":true,"passed":true});
    drop(f);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    write(report);
}

fn kill_executor(project: &Path) -> Vec<String> {
    let output = Command::new("/usr/bin/docker")
        .args([
            "--host",
            "unix:///run/user/2001/docker.sock",
            "ps",
            "-q",
            "--filter",
        ])
        .arg(format!("label=dev.kah.project={}", project.display()))
        .output()
        .unwrap();
    let ids = String::from_utf8(output.stdout).unwrap();
    let ids: Vec<_> = ids.split_whitespace().collect();
    assert!(!ids.is_empty(), "expected project executors");
    assert!(
        Command::new("/usr/bin/docker")
            .args(["--host", "unix:///run/user/2001/docker.sock", "kill"])
            .args(&ids)
            .output()
            .unwrap()
            .status
            .success()
    );
    ids.into_iter().map(str::to_owned).collect()
}

fn write(mut report: Value) {
    report["globalConfigUnchanged"] = true.into();
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/resume-and-loss.json");
    fs::write(path, serde_json::to_string_pretty(&report).unwrap() + "\n").unwrap();
}
