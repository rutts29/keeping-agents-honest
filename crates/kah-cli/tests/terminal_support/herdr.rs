use super::{
    history,
    pty::{Terminal, quote},
};
use crate::fixture;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant, SystemTime},
};

struct Probe {
    root: PathBuf,
    name: String,
}
impl Drop for Probe {
    fn drop(&mut self) {
        // Only the unpredictable named session this fixture created is touched.
        for action in ["stop", "delete"] {
            let _ = Command::new("herdr")
                .args(["session", action, &self.name, "--json"])
                .output();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn run() {
    let home = PathBuf::from(env::var_os("HOME").unwrap());
    let before = fs::read(home.join(".codex/config.toml")).unwrap();
    let (_, mut f) = fixture::prepare(&home, "research", false);
    let probe = setup();
    let command = format!(
        "HERDR_CONFIG_PATH={} herdr --session {}",
        quote(probe.root.join("config.toml").to_str().unwrap()),
        quote(&probe.name)
    );
    let mut terminal = Terminal::start(&command, &f.stderr);
    terminal.wait_for("KAH_HERDR_SHELL_READY");
    terminal.send(&(launch_command(&probe, &f.project) + "\r"));
    terminal.wait_for("Open restricted");
    terminal.send("\r");
    let mut reader = history::connect(&home, &f.project);
    let id = history::session(&mut reader, &f.project, &mut terminal);
    f.sessions.push(id.clone());
    let turn = history::completed_turn(&mut reader, &id, None);
    assert_eq!(
        fs::read_to_string(f.project.join("herdr-proof.txt"))
            .unwrap()
            .trim(),
        "HERDR_WORKS"
    );
    let capture = wait_capture(&probe);
    assert_eq!(capture["pane"]["agent_session"]["value"], id);
    assert_eq!(capture["pane"]["agent"], "codex");
    assert!(
        capture["states"]
            .as_array()
            .unwrap()
            .contains(&json!("working"))
    );
    let operations = crate::checks(&turn);
    let offset = terminal.log_size();
    terminal.send("\x04");
    terminal.wait_for_since("KAH_HERDR_SHELL_READY", offset);
    terminal.send(&format!(
        "test \"$HERDR_ENV\" = 1 && herdr session stop {} --json\r",
        quote(&probe.name)
    ));
    terminal.wait_exit(false);
    drop(terminal);
    drop(reader);
    drop(f);
    drop(probe);
    assert_eq!(fs::read(home.join(".codex/config.toml")).unwrap(), before);
    let report = json!({"capture":capture,"nativeTurn":operations,"globalConfigUnchanged":true,"cleanupComplete":true,"passed":true});
    let output = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/gates/evidence/phase-2/herdr-automated.json");
    fs::write(
        output,
        serde_json::to_string_pretty(&report).unwrap() + "\n",
    )
    .unwrap();
}

fn setup() -> Probe {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("kah-gate-{nonce}");
    let root = env::temp_dir().join(&name);
    fs::create_dir(&root).unwrap();
    let shell = root.join("shell");
    fs::write(
        &shell,
        "#!/bin/sh\nexec env PS1=KAH_HERDR_SHELL_READY /bin/bash --noprofile --norc -i\n",
    )
    .unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        root.join("config.toml"),
        format!(
            "onboarding=false\n[terminal]\ndefault_shell={}\n",
            serde_json::to_string(&shell).unwrap()
        ),
    )
    .unwrap();
    Probe { root, name }
}

fn launch_command(probe: &Probe, project: &Path) -> String {
    let executable = env::current_exe().unwrap();
    let kah = Path::new(env!("CARGO_BIN_EXE_kah"));
    let name = project.file_name().unwrap().to_str().unwrap();
    let prompt = "Use native exec_command once: pwd; test ! -e /home/example/.codex/auth.json && echo HOST_AUTH_HIDDEN; curl --max-time 8 -Is https://example.com; echo CURL_EXIT=$?. Then native apply_patch must create herdr-proof.txt containing HERDR_WORKS. Disposable Herdr test, no other tools, subagents or commits. Report and stop.";
    format!(
        "test \"$HERDR_ENV\" = 1 && (KAH_HERDR_CAPTURE={} KAH_HERDR_MARKER={} {} --exact herdr_capture_entrypoint --nocapture > {} 2>&1 &) && {} research {} --model gpt-6-sol --effort low -- --no-alt-screen {}",
        quote(probe.root.join("capture.json").to_str().unwrap()),
        quote(project.join("herdr-proof.txt").to_str().unwrap()),
        quote(executable.to_str().unwrap()),
        quote(probe.root.join("helper.log").to_str().unwrap()),
        quote(kah.to_str().unwrap()),
        quote(name),
        quote(prompt)
    )
}

fn wait_capture(probe: &Probe) -> Value {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(data) = fs::read(probe.root.join("capture.json"))
            && let Ok(value) = serde_json::from_slice::<Value>(&data)
            && value["complete"] == true
        {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "Herdr capture did not finish: {}",
            fs::read_to_string(probe.root.join("helper.log")).unwrap_or_default()
        );
        thread::sleep(Duration::from_millis(200));
    }
}

pub fn capture() {
    assert_eq!(env::var("HERDR_ENV").unwrap(), "1");
    let pane = env::var("HERDR_PANE_ID").unwrap();
    let output = PathBuf::from(env::var_os("KAH_HERDR_CAPTURE").unwrap());
    let marker = PathBuf::from(env::var_os("KAH_HERDR_MARKER").unwrap());
    let mut states = BTreeSet::new();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let result = Command::new("herdr")
            .args(["pane", "get", &pane])
            .output()
            .unwrap();
        assert!(result.status.success());
        let value: Value = serde_json::from_slice(&result.stdout).unwrap();
        let pane = &value["result"]["pane"];
        let status = pane["agent_status"].as_str().unwrap_or("unknown");
        states.insert(status.to_owned());
        let complete = marker.exists()
            && states.contains("working")
            && matches!(status, "idle" | "done")
            && pane.get("agent_session").is_some();
        fs::write(
            &output,
            serde_json::to_vec(&json!({"pane":pane,"states":states,"complete":complete})).unwrap(),
        )
        .unwrap();
        if complete {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Herdr never observed the completed native session"
        );
        thread::sleep(Duration::from_millis(200));
    }
}
