//! Actual native web.run + kah hook output, scoped to a private full-TUI fixture.
use super::*;
use kah_cli::native_history::History;
use kah_codex::control::Control;
use std::os::unix::fs::PermissionsExt;
#[path = "../journeys/search_records.rs"]
mod search_records;

pub fn run(root: &Path) -> Value {
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-")
    );
    let home = PathBuf::from(std::env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (launch, mut fixture) = prepare(&home, root);
    trace_hook(&home, &launch, root);
    search(&home, root, launch, &mut fixture, &before)
}
fn prepare(home: &Path, root: &Path) -> (kah_cli::Launch, crate::fixture::Fixture) {
    let name = format!("kah-search-{}", std::process::id());
    let args = Arguments::parse(vec![
        "research".into(),
        name,
        "--model".into(),
        "gpt-6.1-sol".into(),
        "--effort".into(),
        "low".into(),
        "--no-observer".into(),
    ])
    .unwrap();
    let launch = kah_cli::Launch::prepare(home, home, args).unwrap();
    kah_codex::overlay::write_settings(&launch.overlay, json!({"service_tier":"fast"})).unwrap();
    let fixture = crate::fixture::Fixture {
        project: launch.project.root.clone(),
        overlay: launch.overlay.clone(),
        canary: root.join("canary"),
        stderr: root.join("codex-stderr"),
        sessions: vec![],
    };
    (launch, fixture)
}
fn search(
    home: &Path,
    root: &Path,
    launch: kah_cli::Launch,
    fixture: &mut crate::fixture::Fixture,
    before: &[u8],
) -> Value {
    let prompt = "Search the web for the official IANA example domains documentation using web.run. Return the official page title and link. No shell commands, MCP, subagents, file changes or commits. If the search tool really fails, report its actual error honestly. Then stop.";
    let mut command = launch.command(false);
    command
        .args(["--no-alt-screen", prompt])
        .env_remove("CODEX_THREAD_ID");
    let panes = Panes(vec![Pane::start(&command).unwrap()]);
    let id = ready_search(&panes.0[0]);
    fixture.sessions.push(id.clone());
    let mut history = History::open(
        home,
        &root.join("inspection"),
        "codex",
        &fixture.project,
        &id,
    )
    .unwrap();
    while !history.completed().unwrap() {
        assert!(
            !panes.0[0].inspect().unwrap()["agent"].is_null(),
            "native search pane lost its agent"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    let History::Codex {
        client, session, ..
    } = &mut history
    else {
        unreachable!()
    };
    let read = client
        .request(
            "thread/read",
            json!({"threadId":session,"includeTurns":false}),
        )
        .unwrap();
    let raw = fs::read_to_string(read["thread"]["path"].as_str().unwrap()).unwrap();
    let rows = raw
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    archive(root, home, &rows);
    let mut report = evidence(root, home, &rows);
    crate::privacy::scrub(&mut report);
    drop(history);
    drop(panes);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    report
}
fn evidence(root: &Path, home: &Path, rows: &[Value]) -> Value {
    let input: Value = serde_json::from_slice(
        &fs::read(root.join("web-hook-input.json")).expect("web.run did not reach kah PreToolUse"),
    )
    .unwrap();
    let output: Value =
        serde_json::from_slice(&fs::read(root.join("web-hook-output.json")).unwrap()).unwrap();
    let (calls, results) = search_records::verify(rows);
    assert_eq!(input["hook_event_name"], "PreToolUse");
    assert!(
        matches!(
            input["tool_name"].as_str(),
            Some("web.run" | "web__run" | "webrun")
        ),
        "trace was not the actual native web tool: {input}"
    );
    assert_ne!(
        output["hookSpecificOutput"]["permissionDecision"], "deny",
        "kah denied web.run: {output}"
    );
    assert!(
        !calls.is_empty() && !results.is_empty(),
        "native web.run call/result missing: {rows:?}"
    );
    assert!(
        results
            .iter()
            .any(|r| r["output"].to_string().contains("iana.org")),
        "search returned no IANA evidence: {results:?}"
    );
    json!({"versions":versions(home),"fixtureModels":{"codex":"gpt-6.1-sol/low"},"domain":"research-offline","fullNativeTui":true,"hookInput":input,"hookOutput":output,"nativeCalls":calls,"nativeResults":results,"passed":true})
}
fn ready_search(pane: &Pane) -> String {
    let mut accepted = false;
    loop {
        let screen = pane.logs("codex", None, None).unwrap();
        if !accepted && screen.contains("Open restricted") {
            crate::native_controller_support::native_consent(&pane.id, "codex");
            accepted = true;
        }
        let view = pane.inspect().unwrap();
        if let Some(id) = view["agent_session"]["value"].as_str() {
            return id.into();
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn trace_hook(home: &Path, launch: &kah_cli::Launch, root: &Path) {
    let script = root.join("web-hook.sh");
    let program = crate::pty::quote(home.join(".local/bin/kah").to_str().unwrap());
    let input = crate::pty::quote(root.join("web-hook-input.json").to_str().unwrap());
    let output = crate::pty::quote(root.join("web-hook-output.json").to_str().unwrap());
    fs::write(&script,format!("#!/bin/sh\nset -eu\numask 077\npayload=$(cat)\nprintf '%s\\n' \"$payload\" > {input}\nreply=$(printf '%s' \"$payload\" | {program} __hook)\nprintf '%s\\n' \"$reply\" > {output}\nprintf '%s\\n' \"$reply\"\n")).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o700)).unwrap();
    trust_trace(home, launch, &script, &program);
}
fn trust_trace(home: &Path, launch: &kah_cli::Launch, script: &Path, program: &str) {
    let path = launch.overlay.join("hooks.json");
    let mut hooks: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let command = crate::pty::quote(script.to_str().unwrap());
    let mut changed = 0;
    for group in hooks["hooks"]["PreToolUse"].as_array_mut().unwrap() {
        for hook in group["hooks"].as_array_mut().unwrap() {
            if hook["command"] == format!("{program} __hook") {
                hook["command"] = command.clone().into();
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1, "replace exactly the actual kah hook");
    fs::write(&path, serde_json::to_vec_pretty(&hooks).unwrap()).unwrap();
    let mut control = Control::connect(
        &home.join(".local/bin/codex"),
        &launch.overlay,
        &launch.project.root,
        Duration::from_secs(10),
    )
    .unwrap();
    let list = control
        .request("hooks/list", json!({"cwds":[launch.project.root]}))
        .unwrap();
    let entry = list["data"][0]["hooks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["sourcePath"] == path.to_str().unwrap()
                && e["eventName"] == "preToolUse"
                && e["command"] == command
        })
        .expect("native logger hook discovered");
    let config = launch.overlay.join("config.toml");
    let mut table: toml::Table = toml::from_str(&fs::read_to_string(&config).unwrap()).unwrap();
    table["hooks"].as_table_mut().unwrap()["state"]
        .as_table_mut()
        .unwrap()
        .insert(
            entry["key"].as_str().unwrap().into(),
            toml::Value::try_from(json!({"enabled":true,"trusted_hash":entry["currentHash"]}))
                .unwrap(),
        );
    fs::write(config, toml::to_string(&table).unwrap()).unwrap();
}
fn archive(root: &Path, home: &Path, rows: &[Value]) {
    let directory = home
        .join(".local/share/kah-eval/hosted-search")
        .join(format!(
            "{}-{}",
            std::process::id(),
            kah_observer::session::now_ms()
        ));
    kah_core::run_store::ensure_private_directory(&directory).unwrap();
    let mut trace = json!({"native":rows,"hook_input":fs::read_to_string(root.join("web-hook-input.json")).ok(),"hook_output":fs::read_to_string(root.join("web-hook-output.json")).ok()});
    crate::privacy::scrub(&mut trace);
    kah_cli::unattended_report::save(&directory, "trace.json", &trace).unwrap();
    eprintln!("private hosted-search trace: {}", directory.display());
}
pub fn web_calls(rows: &[Value]) -> Vec<Value> {
    search_records::calls(rows)
}
#[test]
fn only_an_actual_web_namespace_call_counts_as_search() {
    let rows = vec![
        json!({"type":"message","text":"I searched the web"}),
        json!({"type":"function_call","name":"run","namespace":"functions"}),
        json!({"type":"function_call","name":"run","namespace":"web","call_id":"search"}),
    ];
    let calls = web_calls(&rows);
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0]["call_id"], "search");
}
#[test]
fn recorded_orchestrated_search_retains_call_result_and_actual_hook_decision() {
    let Some(path) = std::env::var_os("KAH_SEARCH_CAPTURE") else {
        return;
    };
    let capture: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let input: Value = serde_json::from_str(capture["hook_input"].as_str().unwrap()).unwrap();
    let output: Value = serde_json::from_str(capture["hook_output"].as_str().unwrap()).unwrap();
    assert_eq!(input["tool_name"], "webrun");
    assert_eq!(output, json!({}));
    let rows = capture["native"].as_array().unwrap();
    let calls = web_calls(rows);
    assert_eq!(calls.len(), 1);
    let result = rows
        .iter()
        .map(|r| &r["payload"])
        .find(|r| r["type"] == "custom_tool_call_output" && r["call_id"] == calls[0]["call_id"])
        .unwrap();
    assert!(
        result["output"]
            .to_string()
            .contains("https://www.iana.org/help/example-domains")
    );
    let context = rows.iter().find(|r| r["type"] == "turn_context").unwrap();
    assert_eq!(context["payload"]["model"], "gpt-6.1-sol");
    assert_eq!(context["payload"]["effort"], "low");
}
