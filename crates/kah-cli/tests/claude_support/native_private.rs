//! The original 5e journey, isolated in native Herdr with kah observation disabled.
use kah_cli::run_pane::Pane;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};

const TASK: &str = "Use the native Agent tool to start one Claude Sonnet 5.5 subagent at low effort with this task: write child.txt containing CHILD_OK in this project. Wait for it to finish. Do not write the file yourself. Then reply CHILD_DONE.";

pub fn entrypoint() {
    let root = std::path::PathBuf::from(std::env::var_os("KAH_TRACK2_ROOT").unwrap());
    let result = std::panic::catch_unwind(|| run(&root)).unwrap_or_else(|_| {
        json!({"passed":false,"reason":"NATIVE5E_FAILURE","retained":root,
            "exit_interstitial_appeared":root.join("exit-interstitial.json").exists(),
            "exit_attempted":root.join("exit-before.json").exists(),
            "jev_calls":0,"observer_enabled":false,"PostToolUse":false,"PostToolUseFailure":false})
    });
    save(&root.join("result.json"), &result);
}

pub fn selected() -> bool {
    if std::env::var("KAH_TRACK2_ENTRYPOINT").as_deref() == Ok("native5e") {
        entrypoint();
        true
    } else if std::env::var_os("KAH_5E_NATIVE_ONLY").is_some() {
        crate::track2_support::launch("native5e");
        true
    } else {
        false
    }
}

fn run(root: &Path) -> Value {
    let home = root.join("home");
    assert_eq!(std::env::var_os("HOME").unwrap(), home.as_os_str());
    assert!(
        std::env::var("HERDR_SOCKET_PATH")
            .unwrap()
            .contains("kah-gate-track2-")
    );
    let project = home.join("repos/mine/fixture-claude");
    let id = format!(
        "00000000-0000-4000-8000-{:012x}",
        kah_observer::session::now_ms()
    );
    save(&root.join("sessions.json"), &json!([id]));
    let transcript_dir = kah_claude::paths::config_root(&home)
        .join("projects")
        .join(kah_claude::paths::project_key(&project));
    let transcript = transcript_dir.join(format!("{id}.jsonl"));
    let pane = start(root, &project, &id);
    let mut resources = Resources::default();
    wait(root, &pane, &mut resources, || {
        let view = pane.inspect().unwrap();
        view["agent"] == "claude"
            // A fresh idle TUI has no durable transcript/session discovery yet.
            // --session-id pins identity; check its durable rows after the task.
            && (view["agent_session"]["value"].is_null()
                || view["agent_session"]["value"] == id)
            && matches!(view["agent_status"].as_str(), Some("idle" | "done"))
    });
    save(&root.join("startup-ready.json"), &pane.inspect().unwrap());
    pane.prompt(TASK).unwrap();
    wait(root, &pane, &mut resources, || {
        crate::claude_support::subagent_completion::completed(&rows(&transcript))
    });
    assert!(
        rows(&transcript)
            .iter()
            .any(|row| { row["sessionId"] == id && row["cwd"] == project.to_str().unwrap() }),
        "NATIVE5E_IDENTITY stage=completion"
    );
    let children = verify_child(
        &project,
        &transcript,
        &transcript_dir.join(&id).join("subagents"),
    );
    exit(root, &pane, &mut resources);
    resources.sample(root);
    let result = json!({"passed":true,"native_only":true,"final_acceptance":false,
        "session":id,"task":TASK,"parent_transcript":transcript,"child_transcripts":children,
        "correlatedChildResultThenParentDoneAndTurnDuration":true,"childFileCreated":true,
        "exit_interstitial_appeared":false,"exit_attempted":true,"exit_status":0,"jev_calls":0,
        "future_hook_set":{"PostToolUse":false,"PostToolUseFailure":false,"observer_enabled":false},
        "resources":resources.value(),"observer_authority_proven":false});
    pane.close().unwrap();
    result
}

fn verify_child(project: &Path, transcript: &Path, directory: &Path) -> Vec<std::path::PathBuf> {
    assert_eq!(
        fs::read_to_string(project.join("child.txt"))
            .unwrap()
            .trim(),
        "CHILD_OK"
    );
    let children = children(directory);
    assert!(
        !children.is_empty(),
        "NATIVE5E_CHILD_TRANSCRIPT stage=completion"
    );
    for path in std::iter::once(transcript).chain(children.iter().map(|p| p.as_path())) {
        check_model(path);
    }
    children
}

fn exit(root: &Path, pane: &Pane, resources: &mut Resources) {
    save(
        &root.join("exit-before.json"),
        &json!({"view":pane.inspect().unwrap(),"screen":screen(pane)}),
    );
    pane.prompt("/exit").unwrap();
    wait(root, pane, resources, || {
        root.join("native-exit-code").exists()
    });
    let status = fs::read_to_string(root.join("native-exit-code")).unwrap();
    assert_eq!(status.trim(), "0", "NATIVE5E_EXIT_STATUS stage=exit");
    save(
        &root.join("exit-after.json"),
        &json!({"view":pane.inspect().unwrap(),"screen":screen(pane),"exit_status":status}),
    );
}

fn start(root: &Path, project: &Path, id: &str) -> Pane {
    let script = root.join("native.sh");
    let args = [
        root.join("home/.local/bin/kah")
            .to_str()
            .unwrap()
            .to_owned(),
        "--no-observer".into(),
        "--agent".into(),
        "claude".into(),
        "--effort".into(),
        "low".into(),
        "--".into(),
        "--session-id".into(),
        id.into(),
        "--permission-mode".into(),
        "acceptEdits".into(),
        "--allowedTools".into(),
        "Bash".into(),
        "Agent".into(),
        "--settings".into(),
        r#"{"disableAllHooks":true,"hooks":{}}"#.into(),
    ];
    let invocation = args
        .iter()
        .map(|s| crate::pty::quote(s))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(
        &script,
        format!(
            "#!/bin/sh\n{invocation}\nnative_status=$?\nprintf '%s\\n' \"$native_status\" > {}\n",
            crate::pty::quote(root.join("native-exit-code").to_str().unwrap())
        ),
    )
    .unwrap();
    save(
        &root.join("invocation.json"),
        &json!({"argv":args,"observer_enabled":false,
        "hook_settings":{"disableAllHooks":true,"hooks":{}},"model":"claude-sonnet-5-5","effort":"low","fastMode":false}),
    );
    let mut command = Command::new("/bin/sh");
    command.arg(script).current_dir(project);
    for key in [
        "KAH_CLAUDE_BINDING",
        "KAH_OBSERVER_BINDING",
        "KAH_SESSION_POLICY",
        "CODEX_THREAD_ID",
    ] {
        command.env_remove(key);
    }
    Pane::start(&command).unwrap()
}

fn wait(root: &Path, pane: &Pane, resources: &mut Resources, condition: impl Fn() -> bool) {
    loop {
        resources.sample(root);
        let visible = screen(pane);
        save(
            &root.join("last-screen.json"),
            &json!({"view":pane.inspect().unwrap(),"screen":visible}),
        );
        if visible.contains("Background work is running") {
            save(
                &root.join("exit-interstitial.json"),
                &json!({"screen":visible,"keys_sent":false}),
            );
            panic!("NATIVE5E_BACKGROUND_INTERSTITIAL stage=exit: retained; no dialog answered");
        }
        assert_ne!(
            pane.inspect().unwrap()["agent_status"],
            "blocked",
            "NATIVE5E_DIALOG stage=native: no keys sent"
        );
        assert!(
            !visible.contains("Security notes:") && !visible.contains("Press Enter to continue"),
            "NATIVE5E_ONBOARDING stage=startup: retained; no dialog answered"
        );
        if condition() {
            return;
        }
        assert!(
            !root.join("native-exit-code").exists(),
            "NATIVE5E_EARLY_EXIT stage=native"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn screen(pane: &Pane) -> String {
    let output = Command::new("herdr")
        .args([
            "pane", "read", &pane.id, "--source", "visible", "--format", "text",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}
fn rows(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|s| serde_json::from_str(s).ok())
        .collect()
}
fn children(directory: &Path) -> Vec<std::path::PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "jsonl"))
        .collect()
}
fn check_model(path: &Path) {
    let assistants = rows(path)
        .into_iter()
        .filter(|r| r["type"] == "assistant")
        .collect::<Vec<_>>();
    assert!(!assistants.is_empty());
    for row in assistants {
        assert_eq!(
            row["message"]["model"], "claude-sonnet-5-5",
            "NATIVE5E_MODEL stage=floor"
        );
        assert!(
            matches!(
                row["effort"].as_str(),
                Some("low" | "medium" | "high" | "max")
            ),
            "NATIVE5E_EFFORT stage=floor"
        );
    }
}
fn save(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

#[derive(Default)]
struct Resources {
    peak_tmp_bytes: u64,
    min_available_ram_kib: Option<u64>,
}
impl Resources {
    fn sample(&mut self, root: &Path) {
        let output = Command::new("df").args(["-B1", "/tmp"]).output().unwrap();
        let line = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .nth(1)
            .unwrap()
            .to_owned();
        let used = line.split_whitespace().nth(2).unwrap().parse().unwrap();
        self.peak_tmp_bytes = self.peak_tmp_bytes.max(used);
        let memory = fs::read_to_string("/proc/meminfo").unwrap();
        let available = memory
            .lines()
            .find(|line| line.starts_with("MemAvailable:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u64>()
            .unwrap();
        self.min_available_ram_kib = Some(
            self.min_available_ram_kib
                .unwrap_or(available)
                .min(available),
        );
        save(&root.join("resources.json"), &self.value());
    }
    fn value(&self) -> Value {
        json!({"peak_tmp_used_bytes":self.peak_tmp_bytes,"min_available_ram_kib":self.min_available_ram_kib,"sample_interval_ms":200})
    }
}
