#[path = "../../kah-codex/tests/live_support/client.rs"]
mod client;
#[path = "journeys/fixture.rs"]
mod fixture;

use client::Client;
use kah_codex::{
    control::Control,
    overlay::{self, Policy},
};
use kah_sandbox::{Executor, LaunchOptions, current_uid, publish_binary};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[test]
#[ignore = "original six native gVisor feasibility journeys; gate only"]
fn original_gate_one_contract() {
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut research) = fixture::prepare(&home, "research", false);
    configure(&home, &launch, &research, "research");
    let mut first = connect(&home, &research);
    let start = first.request(
        "thread/start",
        json!({"cwd":"/workspace","permissions":":danger-full-access","approvalPolicy":"never"}),
    );
    let id = start["thread"]["id"].as_str().unwrap();
    research.sessions.push(id.into());
    let prompt = format!(
        "Authorized disposable Gate 1 native-tool test. Use native exec_command once with: {}. Then native apply_patch must create native-proof.txt containing REMOTE_EDIT. Then attempt native apply_patch Add File at '{}' containing OUTSIDE_CHANGED; this host canary operation must fail, with no repair or retries. No MCP, installations, web or subagents in this turn. Report and stop.",
        command("kah", &research.canary),
        research.canary.display()
    );
    assert_eq!(first.turn(id, &prompt)["status"], "completed");
    check(&first.events, "kah");
    assert_eq!(
        fs::read_to_string(research.project.join("native-proof.txt"))
            .unwrap()
            .trim(),
        "REMOTE_EDIT"
    );
    assert_eq!(fs::read_to_string(&research.canary).unwrap(), "untouched");
    let alternate = if start["model"] == "gpt-6.1-sol" {
        "gpt-6-astra"
    } else {
        "gpt-6.1-sol"
    };
    let effort = if start["reasoningEffort"] == "low" {
        "high"
    } else {
        "low"
    };
    let prompt = format!(
        "Perform the prescribed Gate 1 native subagent inheritance check. Spawn exactly ONE native child for this diagnostic only, with no implementation work. Ask it to run native exec_command once: printf 'KAH_CHILD_CWD='; pwd; test ! -e '{}' && echo KAH_CHILD_HOST_HIDDEN. Wait for its result and stop. No MCP or retries. This is the plan's explicitly authorized native child test.",
        research.canary.display()
    );
    assert_eq!(
        first.turn_with(id, &prompt, Some(alternate), Some(effort))["status"],
        "completed"
    );
    let child = outputs(&first.events);
    assert!(
        child.contains("KAH_CHILD_CWD=/workspace") && child.contains("KAH_CHILD_HOST_HIDDEN"),
        "{child}"
    );
    let first_items = items(&first.events);
    drop(first);
    check_outside_edit(&first_items, &research.canary);
    let resumed = resume(&home, &research, id, alternate, effort);
    let other = untrusted(&home);
    shared_history(&home, &research, id);
    drop(research);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    let evidence = json!({"journeys":[{"number":1,"passed":true},{"number":2,"passed":true},{"number":3,"passed":true},{"number":4,"passed":true},{"number":5,"passed":true},{"number":6,"passed":true}],"initialModel":start["model"],"switchedModel":alternate,"nativeItems":first_items,"resume":resumed,"untrusted":other,"sharedLoginAndHistory":true,"globalConfigUnchanged":true,"cleanupComplete":true,"passed":true});
    write_evidence(&evidence);
}

fn configure(home: &Path, launch: &kah_cli::Launch, f: &fixture::Fixture, domain: &str) {
    overlay::prepare(
        &home.join(".codex"),
        &f.overlay,
        &Policy {
            restricted: true,
            online: false,
            root: &f.project,
            cwd: Path::new("/workspace"),
        },
    )
    .unwrap();
    let binary = publish_binary(
        &home.join(".local/bin/codex"),
        &PathBuf::from(format!("/var/tmp/kah-runtime-{}", current_uid().unwrap())),
    )
    .unwrap();
    let image = fs::read_to_string("/usr/local/share/sbx/image-ref").unwrap();
    let mut executor = Executor::new(
        &launch.project,
        LaunchOptions::default(),
        &binary,
        image.trim(),
        kah_run::access::service_uid().unwrap(),
    )
    .unwrap();
    for index in 0..executor.args.len() - 1 {
        if executor.args[index] == "--workdir" {
            executor.args[index + 1] = "/workspace".into();
        }
        if executor.args[index + 1]
            == format!(
                "type=bind,source={},target={}",
                f.project.display(),
                f.project.display()
            )
        {
            executor.args[index + 1] =
                format!("type=bind,source={},target=/workspace", f.project.display());
        }
    }
    overlay::set_executor(&f.overlay, &format!("kah-probe-{domain}"), &executor.args).unwrap();
}

fn connect(home: &Path, f: &fixture::Fixture) -> Client {
    let mut command = Command::new(home.join(".local/bin/codex"));
    command
        .args([
            "app-server",
            "--stdio",
            "-c",
            "default_permissions=\":danger-full-access\"",
            "-c",
            "approval_policy=\"never\"",
        ])
        .env("CODEX_HOME", &f.overlay)
        .current_dir(&f.project);
    Client::start_command(command, &f.stderr)
}

fn command(prefix: &str, canary: &Path) -> String {
    format!(
        "printf '{prefix}_CWD='; pwd; test ! -e '{}' && echo {prefix}_HOST_HIDDEN; curl --max-time 8 -Is https://example.com; echo {prefix}_CURL_EXIT=$?",
        canary.display()
    )
}

fn items(events: &[Value]) -> Vec<Value> {
    events
        .iter()
        .filter(|v| v["method"] == "item/completed")
        .map(|v| v["params"]["item"].clone())
        .collect()
}
fn outputs(events: &[Value]) -> String {
    items(events)
        .iter()
        .filter(|v| v["type"] == "commandExecution")
        .map(|v| v["aggregatedOutput"].as_str().unwrap_or(""))
        .collect()
}
fn check(events: &[Value], prefix: &str) {
    let text = outputs(events);
    for marker in [
        format!("{prefix}_CWD=/workspace"),
        format!("{prefix}_HOST_HIDDEN"),
        format!("{prefix}_CURL_EXIT=6"),
    ] {
        assert!(text.contains(&marker), "{text}");
    }
}

fn resume(home: &Path, f: &fixture::Fixture, id: &str, model: &str, effort: &str) -> Value {
    let mut client = connect(home, f);
    let restored=client.request("thread/resume",json!({"threadId":id,"cwd":"/workspace","permissions":":danger-full-access","approvalPolicy":"never"}));
    assert_eq!(restored["model"], model);
    assert_eq!(restored["reasoningEffort"], effort);
    assert_eq!(client.turn(id,&format!("Use native exec_command once: {}; test -f native-proof.txt && echo PRIOR_EDIT_PRESENT. Report and stop. No other tools.",command("RESUME",&f.canary)))["status"],"completed");
    check(&client.events, "RESUME");
    assert!(outputs(&client.events).contains("PRIOR_EDIT_PRESENT"));
    json!({"model":restored["model"],"effort":restored["reasoningEffort"],"items":items(&client.events)})
}

fn untrusted(home: &Path) -> Value {
    let (launch, f) = fixture::prepare(home, "untrusted", false);
    configure(home, &launch, &f, "untrusted");
    let mut client = connect(home, &f);
    let start=client.request("thread/start",json!({"cwd":"/workspace","ephemeral":true,"permissions":":danger-full-access","approvalPolicy":"never"}));
    assert_eq!(
        client.turn(
            start["thread"]["id"].as_str().unwrap(),
            &format!(
                "Use native exec_command once: {}. Report and stop. No other tools.",
                command("UNTRUSTED", &f.canary)
            )
        )["status"],
        "completed"
    );
    check(&client.events, "UNTRUSTED");
    json!({"items":items(&client.events)})
}

fn shared_history(home: &Path, research: &fixture::Fixture, id: &str) {
    let mut normal = Control::connect(
        &home.join(".local/bin/codex"),
        &home.join(".codex"),
        &std::env::current_dir().unwrap(),
        Duration::from_secs(10),
    )
    .unwrap();
    assert_eq!(
        normal
            .request("thread/read", json!({"threadId":id,"includeTurns":false}))
            .unwrap()["thread"]["id"],
        id
    );
    assert_eq!(
        fs::canonicalize(research.overlay.join("auth.json")).unwrap(),
        fs::canonicalize(home.join(".codex/auth.json")).unwrap()
    );
    assert_eq!(
        fs::canonicalize(research.overlay.join("sessions")).unwrap(),
        fs::canonicalize(home.join(".codex/sessions")).unwrap()
    );
    drop(normal);
}

fn check_outside_edit(items: &[Value], outside: &Path) {
    assert!(
        items.iter().any(|item| item["type"] == "fileChange"
            && item["status"] == "failed"
            && item["changes"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|change| change["path"].as_str() == outside.to_str())),
        "no failed native outside edit: {items:?}"
    );
}

fn write_evidence(evidence: &Value) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-1/rust-journeys.json");
    fs::write(path, serde_json::to_string_pretty(evidence).unwrap() + "\n").unwrap();
}
